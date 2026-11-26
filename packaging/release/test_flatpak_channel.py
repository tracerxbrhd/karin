# SPDX-License-Identifier: GPL-3.0-or-later
"""Exercise independent OSTree verification using disposable signing keys."""

from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from flatpak_channel import (
    APP_ID, APP_REF, RUNTIME_REF, ostree, trusted_key, verify_signed_bundle,
    verify_signed_pull, write_remote_files,
)


@unittest.skipUnless(all(shutil.which(tool) for tool in ("gpg", "gpgconf", "ostree")),
                     "Real OSTree and GnuPG tools are required")
class SignedChannelTests(unittest.TestCase):
    def setUp(self):
        # GnuPG agent Unix socket addresses have a short path limit.
        temporary = tempfile.TemporaryDirectory(prefix="karin-gpg-test-", dir="/tmp")
        self.addCleanup(temporary.cleanup)
        self.work = Path(temporary.name)
        self.repo = self.work / "remote"
        self.run_tool(ostree(self.repo, "init", "--mode=archive"))
        tree = self.work / "tree"
        tree.mkdir()
        (tree / "content").write_text("Karin isolated signed OSTree test content\n")
        self.commit = self.run_tool(
            ostree(self.repo, "commit", f"--branch={APP_REF}", f"--tree=dir={tree}",
                   "--subject=Test release", "--timestamp=2025-01-01T00:00:00Z"),
        ).strip()
        self.home, self.fingerprint, self.key = self.make_key("release")

    def run_tool(self, args):
        return subprocess.check_output(args, text=True, stderr=subprocess.PIPE)

    def make_key(self, label):
        home = self.work / f"{label}-gpg"
        home.mkdir(mode=0o700)
        self.addCleanup(subprocess.run, ["gpgconf", "--homedir", str(home), "--kill", "gpg-agent"],
                        check=False, capture_output=True)
        gpg = ["gpg", "--batch", "--no-options", "--homedir", str(home)]
        self.run_tool([
            *gpg, "--pinentry-mode", "loopback", "--passphrase", "",
            "--quick-gen-key", f"Karin isolated {label} test", "ed25519", "sign", "0",
        ])
        output = self.run_tool([*gpg, "--with-colons", "--fingerprint", "--list-keys"])
        fingerprint = next(line.split(":")[9] for line in output.splitlines()
                           if line.startswith("fpr:"))
        key = self.work / f"{label}.gpg"
        key.write_bytes(trusted_key(home, fingerprint))
        return home, fingerprint, key

    def verify(self, label, key):
        directory = self.work / label
        directory.mkdir()
        verify_signed_pull(self.repo, directory, key, self.commit)

    def test_pull_requires_both_signatures_from_the_pinned_key(self):
        self.run_tool(ostree(self.repo, "summary", "--update"))
        with self.assertRaises(subprocess.CalledProcessError):
            self.verify("unsigned-summary", self.key)

        self.run_tool(ostree(self.repo, "summary", "--update",
                             f"--gpg-sign={self.fingerprint}", f"--gpg-homedir={self.home}"))
        with self.assertRaises(subprocess.CalledProcessError):
            self.verify("unsigned-deployment", self.key)

        self.run_tool(ostree(self.repo, "gpg-sign", f"--gpg-homedir={self.home}",
                             self.commit, self.fingerprint))
        self.verify("authentic", self.key)

        _, _, other_key = self.make_key("unrelated")
        with self.assertRaises(subprocess.CalledProcessError):
            self.verify("untrusted", other_key)

    @unittest.skipUnless(all(shutil.which(tool) for tool in ("flatpak", "dbus-run-session")),
                         "Stock Flatpak installation tools are required")
    def test_bundle_requires_a_signature_and_configures_the_same_update_origin(self):
        tree = self.work / "tree"
        metadata = f"[Application]\nname={APP_ID}\nruntime={RUNTIME_REF}\ncommand=karin\n"
        (tree / "metadata").write_text(metadata, encoding="utf-8")
        (tree / "files").mkdir()
        self.commit = self.run_tool(
            ostree(self.repo, "commit", f"--branch={APP_REF}", f"--tree=dir={tree}",
                   f"--add-metadata-string=xa.metadata={metadata}", "--subject=Bundle fixture"),
        ).strip()
        channel = self.repo.as_uri()

        def bundle(label, key):
            path = self.work / f"{label}.flatpak"
            self.run_tool(["flatpak", "build-bundle", "--arch=x86_64", f"--repo-url={channel}",
                           f"--gpg-keys={key}", str(self.repo), str(path), APP_ID, "master"])
            destination = self.work / label
            destination.mkdir()
            return path, destination

        unsigned, destination = bundle("unsigned-bundle", self.key)
        with self.assertRaises(subprocess.CalledProcessError):
            verify_signed_bundle(unsigned, destination, self.key, self.commit, channel)

        self.run_tool(ostree(self.repo, "gpg-sign", f"--gpg-homedir={self.home}",
                             self.commit, self.fingerprint))
        _, _, other_key = self.make_key("bundle-unrelated")
        untrusted, destination = bundle("wrong-bundle-key", other_key)
        with self.assertRaises(subprocess.CalledProcessError):
            verify_signed_bundle(untrusted, destination, self.key, self.commit, channel)

        authentic, destination = bundle("authenticated-bundle", self.key)
        result = verify_signed_bundle(authentic, destination, self.key, self.commit, channel)
        self.assertEqual(result["url"], channel)
        self.assertEqual(result["key_fingerprint"], self.fingerprint)
        self.assertEqual(result["runtime"], RUNTIME_REF)

class ChannelPolicyTests(unittest.TestCase):
    def test_release_and_pages_share_canonical_installer_names(self):
        with tempfile.TemporaryDirectory() as directory:
            packages = Path(directory) / "packages"
            repo = Path(directory) / "flatpak"
            packages.mkdir()
            repo.mkdir()
            write_remote_files(packages, repo, b"fixture public certificate")
            names = {"karin.flatpakref", "karin.flatpakrepo"}
            self.assertEqual({path.name for path in packages.iterdir()}, names)
            for name in names:
                self.assertEqual((packages / name).read_bytes(), (repo / name).read_bytes())

    def test_empty_or_malformed_pin_stops_before_keyring_access(self):
        for value in ("", "not a key", "a" * 40, "F" * 64):
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, "pinned"):
                trusted_key(Path("/unused"), value)
