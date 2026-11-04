#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Read only one legacy GTK4 boolean; never print paths/configuration contents.

This reports declarations, not GTK's effective source/precedence or a writer.
It does not change KDE, GTK, Karin preferences or environment variables.
"""
import configparser
import os
from pathlib import Path

key = 'gtk-application-prefer-dark-theme'
config_home = Path(os.environ.get('XDG_CONFIG_HOME') or Path.home() / '.config')
locations = [('user-gtk4', config_home / 'gtk-4.0/settings.ini'),
             ('etc-gtk4', Path('/etc/gtk-4.0/settings.ini')),
             ('share-gtk4', Path('/usr/share/gtk-4.0/settings.ini'))]
for index, directory in enumerate((os.environ.get('XDG_CONFIG_DIRS') or '/etc/xdg').split(':')[:8]):
    if directory:
        locations.append((f'xdg-system-{index}', Path(directory) / 'gtk-4.0/settings.ini'))
for origin, path in locations:
    try:
        with path.open('r', encoding='utf-8') as source:
            text = source.read(65537)
        if len(text) > 65536:
            result = 'size-bound'
        else:
            parser = configparser.ConfigParser(interpolation=None)
            parser.read_string(text)
            value = parser.getboolean('Settings', key, fallback=None)
            result = 'absent' if value is None else ('true' if value else 'false')
    except FileNotFoundError:
        result = 'absent-file'
    except (OSError, UnicodeError, configparser.Error, ValueError):
        result = 'unreadable-or-invalid'
    print(f'theme-input origin={origin} declared-legacy-dark={result}')
print('theme-input scope=file-declarations-only no-files-changed=true')
