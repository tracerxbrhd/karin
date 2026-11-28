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
NAVIGATION_TABS = ("Главная", "Моя коллекция", "Плейлисты")
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("--out", required=True, type=Path)
parser.add_argument("--theme", choices=["light", "dark"], default="light")
parser.add_argument("--expected-revision")
parser.add_argument(
    "--binary-artifact",
    type=Path,
    help="Actual ELF when binary is an installed-package launcher",
)
parser.add_argument("--separate-accessibility-bus", action="store_true")
parser.add_argument("--registry", default="/usr/libexec/at-spi2-registryd")
parser.add_argument(
    "--welcome-only", action="store_true", help="Drive signed-out startup and cancel login"
)
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
# Exercise the retired style value through the ordinary settings loader.
# This is a migration fixture, never an available presentation.
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
                "ui_style": "liquid-glass",
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


def tree(root=None, *, depth_first=False):
    queue = deque(
        [root or ("org.a11y.atspi.Registry", "/org/a11y/atspi/accessible/root")]
    )
    result = []
    visited = set()

    def add_children(ref):
        children = call(ref, ACCESS, "GetChildren")[0]
        if depth_first:
            queue.extendleft(reversed(children))
        else:
            queue.extend(children)

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
            add_children(ref)
        except GLib.Error:
            try:
                add_children(ref)
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


def track_row(title, location):
    return wait(
        lambda: next(
            (
                item
                for item in tree()
                if visible(item)
                and title in item["name"]
                and item["role"] in ("list item", "list box row", "button")
            ),
            None,
        ),
        f"track {title} in {location}",
    )


def open_liked_collection():
    click(widget("Моя коллекция", "toggle button"))
    click(widget("Мне нравится", "button"))

    def full_liked_visible():
        controls = {(item["name"], item["role"]) for item in tree() if visible(item)}
        return ("Мне нравится", "label") in controls and (
            "Мне нравится", "button"
        ) not in controls

    wait(full_liked_visible, "full liked page replaced the collection overview")


def input_(*values):
    subprocess.run(
        ["xdotool", *map(str, values)], check=True, timeout=5, stdout=subprocess.DEVNULL
    )


def click(item):
    # GTK 4.14/4.18 omit screen coordinates and GrabFocus. Use the same
    # accessible Click an assistive user sends to the native actionable child.
    actions = call(item["ref"], "org.a11y.atspi.Action", "GetActions")[0]
    if item["role"] == "button":
        # GtkMenuButton can expose its *menu entries* as outer actions. Index0
        # may enqueue a track rather than open the menu. Activate only this
        # button's own native toggle, never a global same-name match.
        toggles = [
            child
            for child in tree(item["ref"])
            if visible(child)
            and child["role"] == "toggle button"
            and child["name"] == item["name"]
        ]
        assert len(toggles) <= 1, "Ambiguous native MenuButton toggle"
        if toggles:
            item = toggles[0]
        else:
            assert actions, "Button has no native action or owned toggle"
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


def current_info_menu():
    # Row menus intentionally share this visible action label. Start at the
    # current Info heading and take its closest owning pane, never the window.
    ref = widget("О треке", "label")["ref"]
    scopes = []
    for depth in range(6):
        ref = prop(ref, ACCESS, "Parent")
        items = tree(ref)
        menus = [
            item
            for item in items
            if visible(item)
            and item["role"] == "button"
            and item["name"] == "Действия с треком"
        ]
        panel_close = any(
            visible(item)
            and item["name"] in ["Закрыть правую панель", "Закрыть сведения о треке"]
            for item in items
        )
        scopes.append(
            dict(ancestor=depth + 1, menu_count=len(menus), panel_close=panel_close)
        )
        (out / "native-info-scope.json").write_text(json.dumps(scopes, indent=2) + "\n")
        if menus:
            assert (
                len(menus) == 1 and panel_close
            ), "Info pane menu ownership is ambiguous"
            return menus[0]
    raise AssertionError("Current Info pane has no owned native menu")


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


def system_style_control():
    return next(
        (item for item in tree() if visible(item)
         and item["role"] in ["radio", "radio button", "check box", "checkbox"]
         and item["name"] in ["Системный", "Стиль интерфейса: системный"]),
        None,
    )


def check_presentation():
    system = wait(system_style_control, "native System presentation")
    assert is_checked(system), "removed saved style did not fall back to System"
    karin = widget("Стиль интерфейса: Karin")
    assert not karin["states"][0] & (1 << 24), "unfinished Karin became available"
    assert not find("Liquid Glass"), "retired presentation remained in Preferences"


def close_preferences():
    key("Escape")
    wait(lambda: system_style_control() is None, "native Preferences closed")


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


def focused_search():
    def ready():
        entry = find("Поиск музыки", "search box")
        return entry and call(entry["ref"], ACCESS, "GetState")[0][0] & (1 << 12)

    wait(ready, "native Search keyboard focus")


def exercise_system_preferences():
    playerctl("volume", "0.43")
    wait(lambda: abs(media_property("Volume") - 0.43) < 0.001, "confirmed volume")
    before = media_snapshot()
    assert before["status"] == "Playing"
    key("ctrl+comma")
    check_presentation()
    capture("04a-system-preferences")
    close_preferences()
    widget("LRCLIB", "label")
    after = media_snapshot()
    assert after["occurrence"] == before["occurrence"]
    assert after["status"] == "Playing"
    assert after["position"] >= before["position"] - 200_000
    assert abs(after["volume"] - before["volume"]) < 0.001
    assert (after["previous"], after["next"]) == (before["previous"], before["next"])


def exercise_welcome(window_id):
    heading = "Ваша музыка — в Karin"
    sign_in = "Войти через Яндекс"
    scale = int(env.get("GDK_SCALE", "1"))
    for width, height, name in [(420, 720, "compact"), (1440, 900, "wide")]:
        input_("windowsize", "--sync", window_id, width * scale, height * scale)
        widget(heading, "label")
        widget("Войдите в аккаунт Яндекса, чтобы открыть свою музыкальную библиотеку")
        widget(sign_in, "button")
        assert not any(find(name, "toggle button") for name in NAVIGATION_TABS)
        assert not find("Поиск музыки", "search box")
        capture(f"welcome-{name}")

    key("ctrl+f")
    key("alt+2")
    widget(heading, "label")
    assert not find("Поиск музыки", "search box")
    assert not find("Диагностика", "label")
    click(widget("Меню приложения"))
    menu = widget("Меню приложения", "menu")
    items = [item for item in tree(menu["ref"]) if visible(item) and item["role"] == "menu item"]
    assert len(items) == 5, "Signed-out menu must expose updates, preferences, shortcuts, About and Quit"
    key("Escape")

    login = widget(sign_in, "button")
    def focused():
        return bool(call(login["ref"], ACCESS, "GetState")[0][0] & (1 << 12))

    for _ in range(12):
        if focused():
            break
        key("Tab")
    assert focused(), "Welcome sign-in must be reachable by keyboard"
    key("Return")
    widget("Локальная проверка интерфейса: сетевой вход отключён.")
    capture("welcome-offline-login")
    click(widget("Закрыть", "button"))
    wait(lambda: not find("Локальная проверка интерфейса: сетевой вход отключён.") and focused(),
         "login sheet closed and welcome focus restored")
    key("ctrl+q")
    assert app.wait(timeout=12) == 0, "ordinary close failed"



record = []
try:
    if registry is not None:
        subprocess.run(
            ["gdbus", "wait", "--session", "--timeout=5", "org.a11y.atspi.Registry"],
            check=True,
        )
    app = subprocess.Popen(
        [str(binary), "--ui-fixture" if args.welcome_only else "--application-fixture"],
        env={**env, "ADW_DEBUG_COLOR_SCHEME": f"prefer-{args.theme}"} if args.welcome_only else env,
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
                "^Karin$" if args.welcome_only else "^Karin — original local acceptance fixture$",
            ],
            text=True,
            stdout=subprocess.PIPE,
        )
        .stdout.strip()
        .splitlines(),
        "window",
    )[0]
    input_("windowfocus", "--sync", window)
    if args.welcome_only:
        exercise_welcome(window)
        record = dict(result="PASS", fixture="signed-out welcome and offline login", version=version,
                      driver_sha256=driver_sha256, theme=args.theme, scale=env.get("GDK_SCALE", "1"))
        (out / "receipt.json").write_text(json.dumps(record, ensure_ascii=False, indent=2) + "\n")
        print(json.dumps(record, ensure_ascii=False))
        raise SystemExit(0)
    widget("Моя коллекция", "toggle button")
    capture("00-startup")
    # Release metadata must reach the actual native About dialog, not just CLI.
    click(widget("Меню приложения"))
    about_menu = widget("Меню приложения", "menu")
    # Traverse nested menu sections in native visual order before selecting About.
    menu_items = [item for item in tree(about_menu["ref"], depth_first=True)
                  if visible(item) and item["role"] == "menu item"]
    assert len(menu_items) == 8, "Unexpected application menu structure"
    assert all(item["name"] != "Диагностика" for item in menu_items)
    target = menu_items[-2]["ref"]  # About precedes Quit in this menu.
    assert all(item["states"][0] & (1 << 24) for item in menu_items[-2:]), "About/Quit unavailable"

    # GTK 4.14 exports empty model-button names. Follow actual native focus;
    # disabled actions are skipped by GTK rather than counted as key presses.
    def menu_focus():
        return next((item["ref"] for item in menu_items
                     if call(item["ref"], ACCESS, "GetState")[0][0] & (1 << 12)), None)

    for _ in range(len(menu_items)):
        focused = wait(menu_focus, "application menu focus")
        if focused == target:
            break
        key("Down")
        wait(lambda: not call(focused, ACCESS, "GetState")[0][0] & (1 << 12), "next application menu item")
    assert call(target, ACCESS, "GetState")[0][0] & (1 << 12), "About did not receive native focus"
    key("Return")
    application_version = version.splitlines()[0].removeprefix("Karin ")
    widget(application_version)
    widget("Karin contributors")
    capture("00b-about")
    key("Escape")
    wait(lambda: not find(application_version), "native About closed")
    click(widget("Моя коллекция", "toggle button"))
    widget("Моя коллекция", "label")
    widget("Мне нравится", "button")
    track_row("Original river song", "collection overview")
    playlist = "Открыть плейлист: Original fixture playlist · 2 трека"
    widget(playlist, "button")
    capture("01a-collection")
    click(widget("Мои плейлисты", "button"))
    widget("Понравившиеся плейлисты", "label")
    widget(playlist, "button")
    open_liked_collection()
    # The actual library has arrived from the authenticated loopback request.
    row = track_row("Original river song", "liked page")
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
    capture("04-lrc-after-natural-transition")
    exercise_system_preferences()
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
    click(current_info_menu())
    # GTK model-button menu items expose no Name on these runtime versions.
    # The Info menu has the Wave action, three queue actions, Copy Link and Lyrics.
    # Prove its six mapped entries, save its actual painted labels, and drive
    # that native menu with the keyboard as a user does.
    wait(
        lambda: len([x for x in tree() if visible(x) and x["role"] == "menu item"])
        == 6,
        "current Info menu",
    )
    capture("04b-info-menu")
    key("Home")
    for _ in range(5):
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
        and x["name"] in NAVIGATION_TABS
    ]
    assert not tabs, "navigation tabs remained visible beside the field"
    assert search["bounds"][2] < 500, "search expanded over header"
    capture("05-search-open")
    typed("river")
    key("Return")
    wait(
        lambda: log_contains("operation=search outcome=success"),
        "search results through MusicService fixture",
    )
    capture("06-search-results")
    key("ctrl+a")
    key("BackSpace")
    click(widget("Поиск музыки", "search box"))
    assert find("Закрыть поиск", "button"), "empty field unexpectedly closed"
    key("Escape")
    widget("Моя коллекция", "toggle button")
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
        int(geom["X"]) + int(geom["WIDTH"]) // 2,
        int(geom["Y"]) + 120 * int(env.get("GDK_SCALE", "1")),
        "click",
        "1",
    )
    widget("Моя коллекция", "toggle button")
    capture("08-search-outside-closed")
    key("ctrl+q")
    assert app.wait(timeout=12) == 0, "ordinary close failed"
    saved = json.loads(settings.read_text())
    assert (
        saved["preferences"]["lyrics_external_auto"] is True
    ), "explicit preference was not saved"
    assert saved["preferences"]["ui_style"] == "system", "legacy style was not normalized"
    assert saved["preferences"]["appearance"] == args.theme, "color scheme changed"
    assert saved["preferences"]["remember_login"] is False, "login preference changed"
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
    check_presentation()
    capture("09a-restart-style-preference")
    close_preferences()
    open_liked_collection()
    row = track_row("Original river song", "liked page after restart")
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
        native_about_version_checked=True,
        remembered_consent_after_restart=True,
        theme=args.theme,
        style="system",
        legacy_style_migrated=True,
        system_and_reserved_karin_checked=True,
        color_scheme_preserved_after_restart=True,
        preferences_preserve_playback=True,
        user_events="native AT-SPI Click + XTest input",
        original_local_audio=True,
        yandex_text_then_explicit_consent_then_lrclib=True,
        search_uses_music_service_fixture=True,
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
