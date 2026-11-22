#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Drive the ordinary Karin executable through its explicit local composition.

Uses native AT-SPI discovery and accessible Click events, plus actual XTest
keyboard, typing and backdrop-pointer events. No controller or Gtk test API.
The executable owns all synthetic HTTP/audio. No credentials or live account.
Run inside dbus-run-session + Xvfb; at-spi2-registryd must be available.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
from collections import deque
from gi.repository import Gio, GLib

ACCESS = "org.a11y.atspi.Accessible"
COMPONENT = "org.a11y.atspi.Component"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("--out", required=True, type=Path)
parser.add_argument("--theme", choices=["light", "dark"], default="light")
parser.add_argument(
    "--style",
    choices=["system", "liquid-glass"],
    default="system",
    help="Final live-selected style, also verified after an ordinary restart",
)
parser.add_argument("--expected-revision")
parser.add_argument(
    "--binary-artifact",
    type=Path,
    help="Actual ELF when binary is an installed-package launcher",
)
parser.add_argument("--separate-accessibility-bus", action="store_true")
parser.add_argument("--registry", default="/usr/libexec/at-spi2-registryd")
args = parser.parse_args()
driver_sha256 = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
out = args.out.resolve()
out.mkdir(mode=0o700, parents=True, exist_ok=False)
env = os.environ.copy()
for kind in ["config", "cache", "data", "state", "runtime"]:
    directory = out / kind
    directory.mkdir(mode=0o700)
    key = "XDG_RUNTIME_DIR" if kind == "runtime" else f"XDG_{kind.upper()}_HOME"
    env[key] = str(directory)
env.update(
    GTK_A11Y="atspi",
    AT_SPI_BUS_ADDRESS=os.environ["DBUS_SESSION_BUS_ADDRESS"],
    KARIN_LYRICS_DIAGNOSTICS="1",
    G_DEBUG="fatal-warnings",
)
# Seed the actual versioned public preference, through the normal loader.
profile_marker = out / "config" / ".karin-original-fixture"
profile_marker.write_text("original-local-fixture-v1\n")
profile_marker.chmod(0o600)
settings = out / "config" / "io.github.tracerxbrhd.Karin" / "preferences.json"
settings.parent.mkdir(mode=0o700)
settings.write_text(
    json.dumps(
        {
            "version": 3,
            "preferences": {
                "ui_style": "system",
                "appearance": args.theme,
                "remember_login": False,
                "lyrics_external_auto": False,
            },
        }
    )
)
settings.chmod(0o600)
application_log = out / "application.log"
binary = args.binary.resolve()
version = subprocess.check_output([str(binary), "--version"], text=True)
assert (
    "Source commit: " in version and "Profile: " in version and "Package: " in version
)
if args.expected_revision:
    assert f"Source commit: {args.expected_revision}" in version
    assert "Source state: clean" in version
(out / "version.txt").write_text(version)
session_bus = Gio.bus_get_sync(Gio.BusType.SESSION)
if args.separate_accessibility_bus:
    address = session_bus.call_sync(
        "org.a11y.Bus",
        "/org/a11y/bus",
        "org.a11y.Bus",
        "GetAddress",
        None,
        None,
        Gio.DBusCallFlags.NONE,
        5000,
        None,
    ).unpack()[0]
    env["AT_SPI_BUS_ADDRESS"] = address
    bus = Gio.DBusConnection.new_for_address_sync(
        address,
        Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT
        | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
        None,
        None,
    )
    registry = None
else:
    bus = session_bus
    registry = subprocess.Popen(
        [args.registry],
        env=env,
        stdout=(out / "registry.log").open("w"),
        stderr=subprocess.STDOUT,
    )
app = None


def call(ref, interface, method, params=None):
    return bus.call_sync(
        ref[0],
        ref[1],
        interface,
        method,
        params,
        None,
        Gio.DBusCallFlags.NONE,
        1000,
        None,
    ).unpack()


def prop(ref, interface, name):
    return call(
        ref,
        "org.freedesktop.DBus.Properties",
        "Get",
        GLib.Variant("(ss)", (interface, name)),
    )[0]


def tree(root=None):
    queue = deque(
        [root or ("org.a11y.atspi.Registry", "/org/a11y/atspi/accessible/root")]
    )
    result = []
    visited = set()
    while queue and len(visited) < 700:
        ref = tuple(queue.popleft())
        if not ref[0] or ref in visited:
            continue
        visited.add(ref)
        try:
            name = prop(ref, ACCESS, "Name")
            role = call(ref, ACCESS, "GetRoleName")[0]
            bounds = call(ref, COMPONENT, "GetExtents", GLib.Variant("(u)", (0,)))[0]
            states = call(ref, ACCESS, "GetState")[0]
            result.append(
                dict(ref=ref, name=name, role=role, bounds=bounds, states=states)
            )
            queue.extend(call(ref, ACCESS, "GetChildren")[0])
        except GLib.Error:
            try:
                queue.extend(call(ref, ACCESS, "GetChildren")[0])
            except GLib.Error:
                pass
    return result


def visible(item):
    x, y, w, h = item["bounds"]
    return (
        x > -10000
        and y > -10000
        and w > 0
        and h > 0
        and bool(item["states"][0] & (1 << 25))
    )


def wait(predicate, label, seconds=14):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if app is not None and app.poll() is not None:
            raise AssertionError(
                f"Application exited during {label}; inspect application.log"
            )
        value = predicate()
        if value:
            return value
        time.sleep(0.04)
    (out / "failed-tree.json").write_text(
        json.dumps(tree(), ensure_ascii=False, indent=2)
    )
    raise AssertionError(f"Native user operation timed out: {label}")


def find(name, role=None):
    return next(
        (
            x
            for x in tree()
            if x["name"] == name and (role is None or x["role"] == role) and visible(x)
        ),
        None,
    )


def widget(name, role=None):
    return wait(lambda: find(name, role), name)


def input_(*values):
    subprocess.run(
        ["xdotool", *map(str, values)], check=True, timeout=5, stdout=subprocess.DEVNULL
    )


def click(item):
    # GTK 4.14/4.18 omit screen coordinates and GrabFocus. Use the same
    # accessible Click an assistive user sends to the native actionable child.
    actions = call(item["ref"], "org.a11y.atspi.Action", "GetActions")[0]
    if not actions and item["role"] == "button":
        item = wait(
            lambda: find(item["name"], "toggle button"),
            "native menu button child",
        )
    if item["role"] == "label":
        parent = prop(item["ref"], ACCESS, "Parent")
        item = next(
            current for current in tree() if tuple(current["ref"]) == tuple(parent)
        )
    if item["role"] == "menu item":
        items = [x for x in tree() if visible(x) and x["role"] == "menu item"]
        index = next(
            i for i, current in enumerate(items) if current["ref"] == item["ref"]
        )
        key("Home")
        for _ in range(index):
            key("Down")
        key("Return")
        return
    assert call(
        item["ref"], "org.a11y.atspi.Action", "DoAction", GLib.Variant("(i)", (0,))
    )[0], item["name"]


def key(value):
    input_("key", "--clearmodifiers", value)


def typed(value):
    input_("type", "--clearmodifiers", "--delay", "3", "--", value)


def capture(name):
    subprocess.run(
        ["import", "-window", window, str(out / f"{name}.png")], check=True, timeout=8
    )


def log_contains(value):
    return value in application_log.read_text(errors="replace")


def media_property(name):
    return session_bus.call_sync(
        "org.mpris.MediaPlayer2.io.github.tracerxbrhd.Karin",
        "/org/mpris/MediaPlayer2",
        "org.freedesktop.DBus.Properties",
        "Get",
        GLib.Variant("(ss)", ("org.mpris.MediaPlayer2.Player", name)),
        None,
        Gio.DBusCallFlags.NONE,
        3000,
        None,
    ).unpack()[0]


def media_snapshot():
    metadata = media_property("Metadata")
    return {
        "occurrence": metadata["mpris:trackid"],
        "status": media_property("PlaybackStatus"),
        "position": media_property("Position"),
        "volume": media_property("Volume"),
        "previous": media_property("CanGoPrevious"),
        "next": media_property("CanGoNext"),
    }


def playerctl(*arguments):
    subprocess.run(
        ["playerctl", "--player=io.github.tracerxbrhd.Karin", *arguments],
        check=True,
        stdout=subprocess.DEVNULL,
        timeout=5,
    )


def is_checked(item):
    return bool(call(item["ref"], ACCESS, "GetState")[0][0] & (1 << 4))


def find_style_control(style):
    # AdwActionRow labels its activatable radio by the visible row title.
    # GTK versions may instead retain the explicit accessible label.
    names = (
        ["Liquid Glass", "Стиль интерфейса: Liquid Glass"]
        if style == "liquid-glass"
        else ["Системный", "Стиль интерфейса: системный"]
    )
    return next(
        (
            item
            for item in tree()
            if visible(item)
            and item["role"] in ["radio", "radio button", "check box", "checkbox"]
            and item["name"] in names
        ),
        None,
    )


def style_control(style):
    return wait(lambda: find_style_control(style), "native style radio: " + style)


def choose_style(style):
    control = style_control(style)
    if not is_checked(control):
        # ActionRow is a real native activation target for the grouped radio.
        # Some GTK versions omit the check button's AT-SPI Action interface.
        ref = tuple(control["ref"])
        activated = False
        for _ in range(6):
            try:
                role = call(ref, ACCESS, "GetRoleName")[0]
                actions = (
                    call(ref, "org.a11y.atspi.Action", "GetActions")[0]
                    if role
                    in [
                        "check box",
                        "checkbox",
                        "radio button",
                        "radio",
                        "list item",
                        "list box row",
                        "listbox row",
                    ]
                    else []
                )
            except GLib.Error:
                actions = []
            if actions:
                assert call(
                    ref,
                    "org.a11y.atspi.Action",
                    "DoAction",
                    GLib.Variant("(i)", (0,)),
                )[0], "native style activation"
                activated = True
                break
            try:
                ref = tuple(prop(ref, ACCESS, "Parent"))
            except GLib.Error:
                break
        if not activated:
            # A group inside separate AdwActionRows has both row and radio
            # focus stops. Arrow keys can move to a row without checking it;
            # activate the actual named target with Enter/Space afterward.
            target_names = (
                ["Liquid Glass", "Стиль интерфейса: Liquid Glass"]
                if style == "liquid-glass"
                else ["Системный", "Стиль интерфейса: системный"]
            )
            group_names = [
                "Системный",
                "Liquid Glass",
                "Стиль интерфейса: системный",
                "Стиль интерфейса: Liquid Glass",
            ]
            for _ in range(40):
                focused = next(
                    (
                        item
                        for item in tree()
                        if visible(item)
                        and item["name"] in group_names
                        and item["states"][0] & (1 << 12)
                    ),
                    None,
                )
                if focused and focused["name"] in target_names:
                    key("Return" if focused["role"] == "list item" else "space")
                    break
                if focused:
                    key("Down" if style == "liquid-glass" else "Up")
                else:
                    key("Tab")
                time.sleep(0.04)
            else:
                raise AssertionError("Style radio group was not keyboard reachable")
        wait(lambda: is_checked(style_control(style)), "selected native style radio")
    other = "system" if style == "liquid-glass" else "liquid-glass"
    assert not is_checked(
        style_control(other)
    ), "style radio group selected both choices"
    karin = widget("Стиль интерфейса: Karin")
    assert not (
        karin["states"][0] & (1 << 24)
    ), "unfinished Karin style became sensitive"


def close_preferences():
    key("Escape")
    wait(
        lambda: not find_style_control("liquid-glass"),
        "native Preferences closed",
    )


def queue_summary():
    summary = widget("Понравившиеся треки · 2 трека", "label")
    tools = prop(summary["ref"], ACCESS, "Parent")
    page = prop(tools, ACCESS, "Parent")
    rows = [
        item["name"]
        for item in tree(page)
        if visible(item)
        and item["role"] in ["list item", "list box row", "button"]
        and "Original " in item["name"]
        and "Позиция " in item["name"]
    ]
    assert len(rows) == 2, "Both actual queue rows must be mapped"
    return summary["name"], rows


def search_value():
    entry = widget("Поиск музыки", "search box")
    count = prop(entry["ref"], "org.a11y.atspi.Text", "CharacterCount")
    assert 0 <= count <= 1024, "Unexpected synthetic query length"
    # GTK 4.18 reports CharacterCount=5 but returns empty for GetText(0,-1).
    # Read its actual positive range, preserving the exact native query check.
    return call(
        entry["ref"],
        "org.a11y.atspi.Text",
        "GetText",
        GLib.Variant("(ii)", (0, count)),
    )[0]


search_events = []


def search_evidence(phase):
    # Only this fixed, original local fixture is running. These artifacts
    # describe our typed synthetic query, never a user's account or input.
    items = tree()
    entry = next(
        (
            item
            for item in items
            if visible(item)
            and item["role"] == "search box"
            and item["name"] == "Поиск музыки"
        ),
        None,
    )
    search_events.append(
        dict(
            phase=phase,
            query=search_value() if entry else None,
            entry_focused=bool(entry and entry["states"][0] & (1 << 12)),
            visible_dialogs=sum(
                visible(item) and item["role"] == "dialog" for item in items
            ),
            visible_menus=sum(
                visible(item) and item["role"] == "menu" for item in items
            ),
        )
    )
    (out / "native-search-events.json").write_text(
        json.dumps(search_events, ensure_ascii=False, indent=2) + "\n"
    )


def focused_search():
    def ready():
        entry = find("Поиск музыки", "search box")
        return entry and call(entry["ref"], ACCESS, "GetState")[0][0] & (1 << 12)

    wait(ready, "native Search keyboard focus")


def exercise_live_styles():
    # Ordinary desktop controls establish a non-default value and enough
    # duration for each measured transition, without a test player or clock.
    playerctl("volume", "0.43")
    wait(lambda: abs(media_property("Volume") - 0.43) < 0.001, "confirmed volume")
    transitions = ["liquid-glass", "system", "liquid-glass"]
    if args.style == "system":
        transitions.append("system")
    results = []
    for index, style in enumerate(transitions):
        panel = "queue" if index == 1 else "lyrics"
        if panel == "queue":
            click(widget("Открыть очередь", "button"))
            before_queue = queue_summary()
        else:
            if not find("LRCLIB", "label"):
                click(widget("Текст песни", "toggle button"))
            widget("LRCLIB", "label")
            before_queue = None
        search_open = index == 2
        if search_open:
            key("ctrl+f")
            widget("Поиск музыки", "search box")
            focused_search()
            search_evidence("before-native-typing")
            typed("river")
            key("Return")
            search_evidence("after-native-typing")
            wait(lambda: search_value() == "river", "live style search query")
            wait(
                lambda: log_contains("operation=search outcome=success"),
                "search before style transition",
            )
        target_seconds = 4 if media_property("Position") <= 2_000_000 else 1
        playerctl("position", str(target_seconds))
        wait(
            lambda: target_seconds * 1_000_000 - 200_000
            <= media_property("Position")
            <= target_seconds * 1_000_000 + 800_000,
            "confirmed setup seek",
        )
        before = media_snapshot()
        assert before["status"] == "Playing"
        started = time.monotonic()
        key("ctrl+comma")
        choose_style(style)
        capture(f"04c-style-{index:02}-{style}-preferences")
        close_preferences()
        if search_open:
            search_evidence("after-native-Preferences-close")
        if panel == "queue":
            assert queue_summary() == before_queue, "style changed the queue"
        else:
            widget("LRCLIB", "label")
        if search_open:
            assert search_value() == "river", "style replaced the current search"
        after = media_snapshot()
        elapsed = time.monotonic() - started
        assert after["occurrence"] == before["occurrence"], "style replaced occurrence"
        assert after["status"] == "Playing", "style interrupted playback"
        assert after["position"] >= before["position"] - 200_000, "style reset position"
        assert after["position"] - before["position"] <= (elapsed + 2) * 1_000_000
        assert abs(after["volume"] - before["volume"]) < 0.001, "style reset volume"
        assert (after["previous"], after["next"]) == (
            before["previous"],
            before["next"],
        ), "style changed queue transport capabilities"
        capture(f"04d-style-{index:02}-{style}-{panel}")
        if index == 0:
            # Show the new surface hierarchy on the ordinary Home route as
            # well as the playing Lyrics pane. Navigation must retain audio.
            click(widget("Главная", "toggle button"))
            widget("Моя волна", "label")
            time.sleep(0.25)  # Settle only the native route crossfade for PNG.
            capture("04e-first-liquid-glass-home-playing")
            assert media_property("PlaybackStatus") == "Playing"
            assert media_property("Metadata")["mpris:trackid"] == before["occurrence"]
            widget("LRCLIB", "label")
            click(widget("Нравится", "toggle button"))
        results.append(
            dict(
                style=style,
                panel=panel,
                setup_position_seconds=target_seconds,
                position_before_us=before["position"],
                position_after_us=after["position"],
                elapsed_seconds=round(elapsed, 3),
                occurrence_preserved=True,
                playing_preserved=True,
                queue_capabilities_preserved=True,
                volume_preserved=True,
                search_preserved=search_open,
            )
        )
        if search_open:
            key("ctrl+f")
            key("ctrl+a")
            key("BackSpace")
            key("Escape")
            widget("Нравится", "toggle button")
    (out / "live-style-transitions.json").write_text(
        json.dumps(results, indent=2) + "\n"
    )


record = []
try:
    if registry is not None:
        subprocess.run(
            ["gdbus", "wait", "--session", "--timeout=5", "org.a11y.atspi.Registry"],
            check=True,
        )
    app = subprocess.Popen(
        [str(binary), "--application-fixture"],
        env=env,
        stdout=application_log.open("w"),
        stderr=subprocess.STDOUT,
    )
    window = wait(
        lambda: subprocess.run(
            [
                "xdotool",
                "search",
                "--onlyvisible",
                "--name",
                "^Karin — original local acceptance fixture$",
            ],
            text=True,
            stdout=subprocess.PIPE,
        )
        .stdout.strip()
        .splitlines(),
        "window",
    )[0]
    input_("windowfocus", "--sync", window)
    capture("00-startup")
    click(widget("Нравится", "toggle button"))
    # The actual library has arrived from the authenticated loopback request.
    row = wait(
        lambda: next(
            (
                x
                for x in tree()
                if visible(x)
                and "Original river song" in x["name"]
                and x["role"] in ["list item", "list box row", "button"]
            ),
            None,
        ),
        "library track",
    )
    capture("01-library")
    click(row)
    wait(
        lambda: log_contains("request=original-audio credential-free=true"),
        "real GStreamer source request",
    )
    # Footer Lyrics is reached through an ordinary accessible button.
    lyrics = wait(
        lambda: next(
            (
                x
                for x in tree()
                if visible(x) and x["role"] == "toggle button" and "Текст" in x["name"]
            ),
            None,
        ),
        "footer lyrics button",
    )
    click(lyrics)
    wait(
        lambda: log_contains("stage=data provider=yandex source=yandex type=TEXT"),
        "Yandex TEXT through service",
    )
    external = widget("Найти синхронизированный текст", "button")
    widget("Без синхронизации", "label")
    assert not log_contains("request=lrclib") and not log_contains(
        "stage=request provider=lrclib"
    ), "external request before consent"
    capture("02-text-no-sync")
    click(external)
    cancel = widget("Отмена", "button")
    click(cancel)
    wait(lambda: not find("Искать", "button"), "cancelled consent closed")
    assert not log_contains("request=lrclib") and not log_contains(
        "stage=request provider=lrclib"
    ), "cancelled consent sent metadata"
    click(widget("Найти синхронизированный текст", "button"))
    remember = wait(
        lambda: next(
            (
                x
                for x in tree()
                if visible(x)
                and x["role"] in ["checkbox", "check box"]
                and x["name"].startswith(
                    "Автоматически искать синхронизированный текст"
                )
            ),
            None,
        ),
        "remember external consent",
    )
    capture("02b-explicit-consent")
    # GtkCheckButton exposes its checked state but these GTK releases omit
    # its AT-SPI Action. Reach it through the actual native keyboard chain.
    for _ in range(12):
        if call(remember["ref"], ACCESS, "GetState")[0][0] & (1 << 12):
            break
        key("Tab")
        time.sleep(0.04)
    assert call(remember["ref"], ACCESS, "GetState")[0][0] & (
        1 << 12
    ), "consent checkbox keyboard focus"
    key("space")
    wait(
        lambda: call(remember["ref"], ACCESS, "GetState")[0][0] & (1 << 4),
        "explicit remembered consent checked",
    )
    click(widget("Искать", "button"))
    wait(
        lambda: log_contains("stage=data provider=lrclib source=lrclib type=LRC"),
        "actual LRCLIB parsing",
    )
    wait(
        lambda: log_contains(
            "stage=clock state=playing position=true synchronized=true mapped=true active=true"
        ),
        "actual player attachment",
    )
    # Seek with the system's actual transport, then observe a natural boundary.
    subprocess.run(
        ["playerctl", "--player=io.github.tracerxbrhd.Karin", "position", "0"],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    capture("03-lrc")
    for index in range(22):
        time.sleep(0.1)
        capture(f"04-playing-{index:02}")
    wait(
        lambda: log_contains("stage=paint category=intermediate"),
        "painted size transition",
    )
    wait(
        lambda: log_contains("stage=presentation category=completed"),
        "completed presentation",
    )
    assert not find("Караоке", "label"), "redundant synchronized status remained"
    widget("LRCLIB", "label")
    exercise_live_styles()
    # The current occurrence's ordinary Info menu must bind the same player.
    info = wait(
        lambda: next(
            (
                x
                for x in tree()
                if visible(x)
                and x["role"] == "button"
                and x["name"].startswith("Сведения о треке: Original river song")
            ),
            None,
        ),
        "current track info",
    )
    click(info)
    click(widget("Действия с треком", "button"))
    # GTK model-button menu items expose no Name on these runtime versions.
    # The ordinary Info menu has three queue actions followed by Lyrics.
    # Prove its four mapped entries, save its actual painted labels, and drive
    # that native menu with the keyboard as a user does.
    wait(
        lambda: len([x for x in tree() if visible(x) and x["role"] == "menu item"])
        == 4,
        "current Info menu",
    )
    capture("04b-info-menu")
    key("Home")
    for _ in range(3):
        key("Down")
    key("Return")
    widget("LRCLIB", "label")
    wait(
        lambda: log_contains(
            "stage=clock state=playing position=true synchronized=true mapped=true active=true"
        ),
        "Track Info Lyrics clock",
    )
    capture("04b-info-lyrics")
    # Global search opens one bounded field and preserves the originating list.
    click(widget("Поиск музыки", "button"))
    search = widget("Поиск музыки", "search box")
    focused_search()
    tabs = [
        x
        for x in tree()
        if visible(x)
        and x["role"] == "toggle button"
        and x["name"] in ["Главная", "Нравится", "Плейлисты"]
    ]
    assert not tabs, "navigation tabs remained visible beside the field"
    assert search["bounds"][2] < 500, "search expanded over header"
    capture("05-search-open")
    typed("river")
    key("Return")
    wait(
        lambda: log_contains("operation=search outcome=success"),
        "search results via real service",
    )
    capture("06-search-results")
    key("ctrl+a")
    key("BackSpace")
    click(widget("Поиск музыки", "search box"))
    assert find("Закрыть поиск", "button"), "empty field unexpectedly closed"
    key("Escape")
    widget("Нравится", "toggle button")
    assert not find("Поиск музыки", "search box"), "Escape retained entry"
    capture("07-search-closed")
    key("ctrl+f")
    widget("Поиск музыки", "search box")
    geometry = subprocess.check_output(
        ["xdotool", "getwindowgeometry", "--shell", window], text=True
    )
    geom = dict(line.split("=", 1) for line in geometry.splitlines() if "=" in line)
    input_(
        "mousemove",
        "--sync",
        int(geom["X"]) + int(geom["WIDTH"]) // 2,
        int(geom["Y"]) + 120 * int(env.get("GDK_SCALE", "1")),
        "click",
        "1",
    )
    widget("Нравится", "toggle button")
    capture("08-search-outside-closed")
    key("ctrl+q")
    assert app.wait(timeout=12) == 0, "ordinary close failed"
    saved = json.loads(settings.read_text())
    assert (
        saved["preferences"]["lyrics_external_auto"] is True
    ), "explicit preference was not saved"
    assert saved["preferences"]["ui_style"] == args.style, "live style was not saved"
    # A fresh process with the same private synthetic profile must use that
    # exact consent preference. The account is freshly validated again.
    application_log = out / "application-restart.log"
    app = subprocess.Popen(
        [str(binary), "--application-fixture"],
        env=env,
        stdout=application_log.open("w"),
        stderr=subprocess.STDOUT,
    )
    window = wait(
        lambda: subprocess.run(
            [
                "xdotool",
                "search",
                "--onlyvisible",
                "--name",
                "^Karin — original local acceptance fixture$",
            ],
            text=True,
            stdout=subprocess.PIPE,
        )
        .stdout.strip()
        .splitlines(),
        "restart window",
    )[0]
    input_("windowfocus", "--sync", window)
    key("ctrl+comma")
    assert is_checked(style_control(args.style)), "restart lost the selected style"
    capture("09a-restart-style-preference")
    close_preferences()
    click(widget("Нравится", "toggle button"))
    row = wait(
        lambda: next(
            (
                x
                for x in tree()
                if visible(x)
                and "Original river song" in x["name"]
                and x["role"] in ["list item", "list box row", "button"]
            ),
            None,
        ),
        "restart library track",
    )
    click(row)
    click(widget("Текст песни", "toggle button"))
    wait(
        lambda: log_contains("stage=data provider=lrclib source=lrclib type=LRC"),
        "remembered preference in fresh process",
    )
    assert log_contains(
        "stage=consent remembered=true"
    ), "saved permission was not used"
    assert not find("Искать", "button"), "restart presented a new consent prompt"
    assert not find(
        "Найти синхронизированный текст?"
    ), "restart presented consent again"
    capture("09-restart-automatic-lrc")
    key("ctrl+q")
    assert app.wait(timeout=12) == 0
    record = dict(
        result="PASS",
        driver_sha256=driver_sha256,
        binary_sha256=hashlib.sha256(
            (args.binary_artifact or binary).read_bytes()
        ).hexdigest(),
        version=version,
        ordinary_executable=True,
        remembered_consent_after_restart=True,
        theme=args.theme,
        style=args.style,
        live_style_cycle=True,
        style_preserved_after_restart=True,
        playback_queue_volume_search_panel_preserved=True,
        user_events="native AT-SPI Click + XTest input",
        original_local_audio=True,
        yandex_text_then_explicit_consent_then_lrclib=True,
        search_uses_real_service=True,
        real_account=False,
        physical_fedora=False,
        scale=env.get("GDK_SCALE", "1"),
        renderer=env.get("GSK_RENDERER", "default"),
    )
    (out / "receipt.json").write_text(
        json.dumps(record, ensure_ascii=False, indent=2) + "\n"
    )
    print(json.dumps(record, ensure_ascii=False))
finally:
    if app is not None and app.poll() is None:
        app.terminate()
        try:
            app.wait(timeout=10)
        except subprocess.TimeoutExpired:
            app.kill()
            app.wait()
    if registry is not None:
        registry.terminate()
        try:
            registry.wait(timeout=5)
        except subprocess.TimeoutExpired:
            registry.kill()
            registry.wait()
