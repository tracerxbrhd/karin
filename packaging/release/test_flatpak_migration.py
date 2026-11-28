# SPDX-License-Identifier: GPL-3.0-or-later
"""Real signed, isolated Flatpak metadata migrations, without a runtime or app launch.

These tests exercise the system-manager fallback. A successful CLI transaction
does not establish that UpdatePortal may grant new sandbox permissions. All
repositories, signing keys, installations and data are disposable and local.
"""

import configparser
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from artifacts import validate_permissions
from flatpak_channel import (
    APP_ID, APP_REF, RUNTIME_REF, ostree, trusted_key, verify_signed_bundle,
)
from publication import release_assets


DISCORD_FILESYSTEMS = (
    "xdg-run/discord-ipc-0:ro",
    "xdg-run/app/com.discordapp.Discord:ro",
)


@unittest.skipUnless(all(shutil.which(tool) for tool in (
    "flatpak", "ostree", "gpg", "gpgconf", "dbus-run-session",
)), "Stock Flatpak, OSTree, GnuPG and session bus tools are required")
class SignedMigrationTests(unittest.TestCase):
    def setUp(self):
        # GnuPG socket paths must remain short; never use the user's keyring.
        temporary = tempfile.TemporaryDirectory(prefix="karin-migration-", dir="/tmp")
        self.addCleanup(temporary.cleanup)
        self.work = Path(temporary.name)
        self.repo = self.work / "channel"
        self.run_tool(ostree(self.repo, "init", "--mode=archive"))
        self.home, self.pin, self.key = self.make_key("channel")
        self.env = os.environ.copy()
        # Only disposable child processes see this isolated home. This also
        # gives Flatpak's actual ~/.var/app data path a private owner.
        isolated_home = self.work / "user-home"
        isolated_home.mkdir(mode=0o700)
        self.env["HOME"] = str(isolated_home)
        for variable, name in {
            "FLATPAK_USER_DIR": "installation",
            "FLATPAK_SYSTEM_DIR": "empty-system",
            "XDG_CONFIG_HOME": "config",
            "XDG_DATA_HOME": "data",
            "XDG_CACHE_HOME": "cache",
            "XDG_STATE_HOME": "state",
            "XDG_RUNTIME_DIR": "runtime",
        }.items():
            path = self.work / name
            path.mkdir(mode=0o700)
            self.env[variable] = str(path)

    def run_tool(self, command):
        return subprocess.check_output(command, text=True, stderr=subprocess.PIPE)

    def make_key(self, name):
        home = self.work / f"{name}-gpg"
        home.mkdir(mode=0o700)
        self.addCleanup(subprocess.run, ["gpgconf", "--homedir", str(home), "--kill", "gpg-agent"],
                        check=False, capture_output=True)
        command = ["gpg", "--batch", "--no-options", "--homedir", str(home)]
        self.run_tool([*command, "--pinentry-mode", "loopback", "--passphrase", "",
                       "--quick-gen-key", f"Karin disposable {name}", "ed25519", "sign", "0"])
        listing = self.run_tool([*command, "--with-colons", "--fingerprint", "--list-keys"])
        pin = next(line.split(":")[9] for line in listing.splitlines() if line.startswith("fpr:"))
        key = self.work / f"{name}.gpg"
        key.write_bytes(trusted_key(home, pin))
        return home, pin, key

    def publish(self, version, *, discord=False, tray=False, signing=None):
        home, pin = signing or (self.home, self.pin)
        tree = self.work / f"tree-{version}"
        (tree / "files/bin").mkdir(parents=True)
        # Original inert test content; never represented as Karin's executable.
        (tree / "files/bin/karin").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        (tree / "files/bin/karin").chmod(0o755)
        (tree / "files/fixture-version").write_text(version, encoding="ascii")
        metadata = (
            f"[Application]\nname={APP_ID}\nruntime={RUNTIME_REF}\ncommand=karin\n"
            "[Context]\nshared=network;ipc;\nsockets=wayland;fallback-x11;pulseaudio;\n"
            "devices=dri;\n"
        )
        if discord:
            metadata += f"filesystems={';'.join(DISCORD_FILESYSTEMS)};\n"
        metadata += f"[Session Bus Policy]\norg.mpris.MediaPlayer2.{APP_ID}=own\n"
        if tray:
            metadata += "org.kde.StatusNotifierWatcher=talk\norg.kde.plasmashell=see\n"
        (tree / "metadata").write_text(metadata, encoding="utf-8")
        commit = self.run_tool(ostree(
            self.repo, "commit", f"--branch={APP_REF}", f"--tree=dir={tree}",
            f"--add-metadata-string=xa.metadata={metadata}", f"--subject=Original fixture {version}",
        )).strip()
        self.run_tool(ostree(self.repo, "gpg-sign", f"--gpg-homedir={home}", commit, pin))
        self.run_tool([
            "flatpak", "build-update-repo", "--no-update-appstream", f"--gpg-sign={pin}",
            f"--gpg-homedir={home}", str(self.repo),
        ])
        return commit

    def install_old(self, commit):
        bundle = self.work / "original-metadata.flatpak"
        self.run_tool([
            "flatpak", "build-bundle", "--arch=x86_64", f"--repo-url={self.repo.as_uri()}",
            f"--gpg-keys={self.key}", str(self.repo), str(bundle), APP_ID, "master",
        ])
        # Exercise the production trust/origin verifier first, in its own clean installation.
        check = self.work / "independent-bundle-check"
        check.mkdir()
        verified = verify_signed_bundle(bundle, check, self.key, commit, self.repo.as_uri())
        self.assertEqual(verified["key_fingerprint"], self.pin)
        self.flatpak("install", "--user", "--bundle", "--noninteractive", "--no-deps",
                     "--no-related", str(bundle))
        self.assertEqual(self.info("--show-commit"), commit)

    def flatpak(self, *args, check=True):
        # A disposable session/system bus also works in CI without a system daemon.
        return subprocess.run([
            "dbus-run-session", "--", "sh", "-ec",
            'DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS" exec "$@"',
            "karin-migration-test", "flatpak", *args,
        ], env=self.env, text=True, capture_output=True, check=check)

    def info(self, option):
        return self.flatpak("info", "--user", option, APP_ID).stdout.strip()

    def migrate(self, old_version, new_version, *, old_discord=False, discord=False, tray=False):
        self.assertEqual(release_assets(new_version), {
            f"karin-{new_version}-x86_64.flatpak", f"karin-{new_version}-linux-x86_64.tar.gz",
        })
        original = self.publish(old_version, discord=old_discord)
        self.install_old(original)
        origin = self.info("--show-origin")
        data = Path(self.env["HOME"]) / ".var/app" / APP_ID / "data/original-test-user-data"
        data.parent.mkdir(parents=True)
        data.write_text("Preserve original isolated data\n", encoding="utf-8")
        target = self.publish(new_version, discord=discord, tray=tray)
        self.flatpak("update", "--user", "--noninteractive", "--no-deps", "--no-related", APP_ID)
        self.assertEqual(self.info("--show-commit"), target)
        self.assertEqual(self.info("--show-ref"), APP_REF)
        self.assertEqual(self.info("--show-runtime"), RUNTIME_REF)
        self.assertEqual(self.info("--show-origin"), origin)
        self.assertEqual(data.read_text(), "Preserve original isolated data\n")
        location = Path(self.info("--show-location"))
        self.assertEqual((location / "files/fixture-version").read_text(), new_version)
        permissions = configparser.ConfigParser(interpolation=None)
        installed_permissions = self.info("--show-permissions")
        if tray:
            validate_permissions(installed_permissions)
        else:
            # Historical deployments do not match the current release policy.
            with self.assertRaisesRegex(ValueError, "bus permissions" if discord else "Discord permissions"):
                validate_permissions(installed_permissions)
        permissions.read_string(installed_permissions)
        paths = permissions.get("Context", "filesystems", fallback="").strip(";").split(";")
        self.assertEqual(set(filter(None, paths)), set(DISCORD_FILESYSTEMS) if discord else set())
        expected_bus = {f"org.mpris.mediaplayer2.{APP_ID.lower()}": "own"}
        if tray:
            expected_bus["org.kde.statusnotifierwatcher"] = "talk"
            expected_bus["org.kde.plasmashell"] = "see"
        self.assertEqual(dict(permissions["Session Bus Policy"]), expected_bus)

    def test_same_permission_signed_082_to_083_migration(self):
        self.migrate("0.8.2", "0.8.3")

    def test_system_manager_can_authorize_082_to_083_added_ipc_permissions(self):
        # The user-authorized system transaction is a distinct path from Portal.
        self.migrate("0.8.2", "0.8.3", discord=True)

    def test_system_manager_can_authorize_085_to_086_added_tray_permissions(self):
        self.migrate("0.8.5", "0.9.0", old_discord=True, discord=True, tray=True)

    def test_untrusted_new_channel_signature_cannot_replace_the_old_deployment(self):
        original = self.publish("0.8.2")
        self.install_old(original)
        other_home, other_pin, _ = self.make_key("untrusted")
        target = self.publish("0.8.3", signing=(other_home, other_pin))
        # Stock Flatpak may return success with no update after a failed remote
        # refresh. The admission assertion is the actual installed deployment.
        self.flatpak("update", "--user", "--noninteractive", "--no-deps", "--no-related",
                     APP_ID, check=False)
        self.assertNotEqual(self.info("--show-commit"), target)
        self.assertEqual(self.info("--show-commit"), original)
