# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline artifact regressions; no compiler, network, Git or desktop session."""

from contextlib import ExitStack
import hashlib
import json
from pathlib import Path
import shutil
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import artifacts

REVISION = "a" * 40
METADATA = {"version": "0.8.0", "revision": REVISION,
            "source_status": "clean", "source_date_epoch": 1234567890}
VERSION = "Karin 0.8.0\nCompiler: rustc 1.98.1\nTarget: x86_64-unknown-linux-gnu\n"
PERMISSIONS = """[Context]
shared=network;ipc;
sockets=wayland;x11;fallback-x11;pulseaudio;
devices=dri;
[Session Bus Policy]
org.mpris.MediaPlayer2.io.github.tracerxbrhd.Karin=own
"""


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.repository, self.stage, self.output = (self.root / name for name in ("repo", "source", "output"))
        self.stage.mkdir()
        self.project = {
            "Cargo.toml": b'[package]\nname="karin"\nversion="0.8.0"\n',
            "Cargo.lock": (b'version=4\n[[package]]\nname="karin"\nversion="0.8.0"\n'
                           b'[[package]]\nname="example"\nversion="1.0"\n'
                           b'source="registry+https://github.com/rust-lang/crates.io-index"\n'
                           b'checksum="' + b'f' * 64 + b'"\n'),
            "src/lib.rs": b"original source",
            "LICENSE": b"GPL notice", "docs/LICENSING.md": b"license contracts",
            "docs/BUILDING.md": b"build instructions",
            f"packaging/{artifacts.APP_ID}.desktop": b"desktop metadata",
            f"packaging/{artifacts.APP_ID}.metainfo.xml": b"AppStream metadata",
            **{f"packaging/icons/hicolor/{size}x{size}/apps/{artifacts.APP_ID}.png":
               f"original {size} px icon".encode() for size in artifacts.APP_ICON_SIZES},
            "packaging/third-party-licenses/cassette-auth/COPYING": b"Cassette GPL",
            "packaging/third-party-licenses/cassette-auth/PROVENANCE.md": b"provider provenance",
            "packaging/third-party-licenses/tabler-icons/LICENSE": b"original icon MIT notice",
            "packaging/third-party-licenses/tabler-icons/PROVENANCE.md": b"pinned icon revision",
            "packaging/third-party-licenses/fonts/Montserrat-OFL.txt": b"original Montserrat OFL notice",
            "packaging/third-party-licenses/fonts/Inter-OFL.txt": b"original Inter OFL notice",
            "packaging/third-party-licenses/fonts/PROVENANCE.md": b"pinned font sources",
        }
        generated = {
            "BUILD-REVISION": REVISION.encode(), "BUILD-SOURCE-STATUS": b"clean",
            ".cargo/config.toml": b"locked local vendor configuration",
            "vendor/example-1.0/Cargo.toml": b"original crate manifest",
            "vendor/example-1.0/src/lib.rs": b"original crate source",
            "third-party-notices/example-1.0/LICENSE": b"dependency license",
            "third-party-notices/DEPENDENCY_LICENSES.md": b"complete inventory",
        }
        for name, data in {**self.project, **generated}.items():
            path = self.stage / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        vendor = self.stage / "vendor/example-1.0"
        (vendor / ".cargo-checksum.json").write_text(json.dumps({
            "package": "f" * 64,
            "files": {name: hashlib.sha256((vendor / name).read_bytes()).hexdigest()
                      for name in ("Cargo.toml", "src/lib.rs")}}))
        stack = self.enterContext(ExitStack())
        stack.enter_context(patch.object(artifacts, "identity", return_value=METADATA.copy()))
        stack.enter_context(patch.object(artifacts, "validate_prepared"))
        stack.enter_context(patch.object(artifacts, "runtime_entries", return_value={
            "COPYRIGHT-library.html": b"compiler-matched standard library notices",
            "compiler-builtins/LICENSE.txt": b"MIT and Apache LLVM notices",
            "BUILD-INFO.json": b'{"compiler": "rustc 1.98.1", "target": "x86_64-unknown-linux-gnu"}\n'}))
        stack.enter_context(patch.object(artifacts, "validate_runtime", return_value={"compiler": "rustc 1.98.1"}))
        stack.enter_context(patch.object(artifacts, "committed_files", return_value={name: (data, False) for name, data in self.project.items()}))

    def source(self):
        return artifacts.source_artifact(self.repository, REVISION, self.output, self.stage)

    def test_source_contains_corresponding_source_and_never_private_or_build_extras(self):
        for name in (".env", "private-account.json", "target/release/karin", "cargo-home/token", "vendor/example-1.0/unlisted-secret"):
            path = self.stage / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"PRIVATE-SENTINEL")
        path = self.source()
        with tarfile.open(path, "r:gz") as tar:
            names = set(tar.getnames())
            prefix = "karin-0.8.0/"
            for name in self.project:
                self.assertIn(prefix + name, names)
            self.assertIn(prefix + "vendor/example-1.0/src/lib.rs", names)
            self.assertIn(prefix + "third-party-notices/example-1.0/LICENSE", names)
            self.assertIn(prefix + ".cargo/config.toml", names)
            self.assertTrue(all(b"PRIVATE-SENTINEL" not in tar.extractfile(member).read() for member in tar))
            info = json.loads(tar.extractfile(prefix + "BUILD-INFO.json").read())
            self.assertEqual(info["revision"], REVISION)
            self.assertEqual(info["source_status"], "clean")

    def test_source_refuses_symlink_to_outside_content(self):
        path = self.stage / "src/lib.rs"
        path.unlink()
        secret = self.root / "outside-secret"
        secret.write_bytes(b"private data")
        path.symlink_to(secret)
        with self.assertRaisesRegex(ValueError, "linked"):
            self.source()
        self.assertFalse(list(self.output.glob("*.tar.gz")))

    def test_existing_source_is_reused_only_when_all_bytes_and_modes_match(self):
        path = self.source()
        original = path.read_bytes()
        self.assertEqual(self.source().read_bytes(), original)
        with tarfile.open(path, "r:gz") as tar:
            members = [(m, tar.extractfile(m).read()) for m in tar]
        members[0][0].mode ^= 0o111
        with tarfile.open(path, "w:gz") as tar:
            import io
            for member, data in members:
                tar.addfile(member, io.BytesIO(data))
        with self.assertRaisesRegex(ValueError, "modes"):
            self.source()

    def test_source_refuses_incomplete_vendor_inventory(self):
        marker = self.stage / "vendor/example-1.0/.cargo-checksum.json"
        marker.write_text(json.dumps({"package": "f" * 64, "files": {}}))
        with self.assertRaisesRegex(ValueError, "complete Cargo"):
            self.source()

    def test_native_packages_verified_elf_metadata_icon_and_all_notices(self):
        binary = self.root / "actual-karin"
        binary.write_bytes(b"\x7fELFactual compiled bytes")
        # A prepared copy is not permission to convey arbitrary extra files.
        private = self.stage / "packaging/third-party-licenses/tabler-icons/private-profile"
        private.write_bytes(b"PRIVATE-SENTINEL")
        version = VERSION
        libraries = {"scope": "Packaging-host runtime observations", "pkg_config": {"gtk4": "4.18.6"}, "ldd": "libgtk-4.so.1 => /usr/lib/libgtk-4.so.1", "distribution": "No shared libraries are bundled"}
        with patch.object(artifacts, "verify_binary", return_value=version) as verify, patch.object(artifacts, "native_libraries", return_value=libraries):
            path = artifacts.native_artifact(self.repository, REVISION, binary, self.stage, self.output)
        verify.assert_called_once_with(binary.resolve(), METADATA, "Native")
        with tarfile.open(path, "r:gz") as tar:
            self.assertEqual(tar.extractfile("karin").read(), binary.read_bytes())
            self.assertTrue(tar.getmember("karin").mode & 0o111)
            self.assertTrue(tar.getmember("run-karin").mode & 0o111)
            self.assertIn(b'XDG_DATA_DIRS="$root/share:', tar.extractfile("run-karin").read())
            names = set(tar.getnames())
            self.assertIn(f"share/metainfo/{artifacts.APP_ID}.metainfo.xml", names)
            for size in artifacts.APP_ICON_SIZES:
                icon = f"hicolor/{size}x{size}/apps/{artifacts.APP_ID}.png"
                self.assertEqual(tar.extractfile(f"share/icons/{icon}").read(),
                                 self.project[f"packaging/icons/{icon}"])
            self.assertIn(f"share/licenses/{artifacts.APP_ID}/third-party/example-1.0/LICENSE", names)
            self.assertIn(f"share/licenses/{artifacts.APP_ID}/cassette-auth/PROVENANCE.md", names)
            for component, name in (("tabler-icons", "LICENSE"), ("tabler-icons", "PROVENANCE.md"),
                                    ("fonts", "Montserrat-OFL.txt"), ("fonts", "Inter-OFL.txt"),
                                    ("fonts", "PROVENANCE.md")):
                relative = f"{component}/{name}"
                self.assertEqual(tar.extractfile(f"share/licenses/{artifacts.APP_ID}/{relative}").read(),
                                 self.project[f"packaging/third-party-licenses/{relative}"])
            self.assertNotIn(f"share/licenses/{artifacts.APP_ID}/tabler-icons/private-profile", names)
            self.assertIn(f"share/licenses/{artifacts.APP_ID}/rust-runtime/COPYRIGHT-library.html", names)
            self.assertIn(f"share/licenses/{artifacts.APP_ID}/rust-runtime/compiler-builtins/LICENSE.txt", names)
            self.assertFalse(any(name.endswith(".so") for name in names))
            info = json.loads(tar.extractfile("BUILD-INFO.json").read())
            self.assertEqual(info["binary_sha256"], hashlib.sha256(binary.read_bytes()).hexdigest())
            self.assertEqual(info["binary_version"], version)
            self.assertEqual(info["corresponding_source"]["sha256"], artifacts.sha256(self.output / info["corresponding_source"]["file"]))

    def test_wrong_native_source_identity_stops_before_packaging(self):
        binary = self.root / "karin"
        binary.write_bytes(b"\x7fELFactual bytes")
        with patch.object(artifacts, "verify_binary", side_effect=ValueError("wrong source identity")), patch.object(artifacts, "archive") as archive:
            with self.assertRaisesRegex(ValueError, "wrong source"):
                artifacts.native_artifact(self.repository, REVISION, binary, self.stage, self.output)
        archive.assert_not_called()

    def test_unverified_flatpak_source_stops_before_builder(self):
        manifest = self.root / "manifest.json"
        manifest.write_text("{}")
        with patch.object(artifacts, "provenance", side_effect=ValueError("wrong source archive")), patch.object(artifacts.subprocess, "run") as run:
            with self.assertRaisesRegex(ValueError, "wrong source"):
                artifacts.flatpak_artifact(self.repository, REVISION, manifest, self.output)
        run.assert_not_called()

    def test_sdk_extension_ref_is_selected_by_sdk_metadata_not_another_installed_branch(self):
        sdk = "runtime/org.gnome.Sdk/x86_64/50"
        platform = "runtime/org.gnome.Platform/x86_64/50"
        rust = "runtime/org.freedesktop.Sdk.Extension.rust-stable/x86_64/25.08"
        observed = []

        def capture(command):
            observed.append(command)
            if "--show-ref" in command:
                return sdk if command[-1] == "org.gnome.Sdk//50" else platform
            if "--show-metadata" in command:
                self.assertEqual(command[-1], sdk)
                return "[Extension org.freedesktop.Sdk.Extension]\nsubdirectories=true\nversion=25.08\n"
            if command[-1] in (sdk, platform, rust):
                return "correct-commit"
            if command[-1] == "org.freedesktop.Sdk.Extension.rust-stable":
                return "wrong-newer-26.08-commit"
            raise AssertionError(command)

        manifest = {"runtime": "org.gnome.Platform", "sdk": "org.gnome.Sdk", "runtime-version": "50", "sdk-extensions": ["org.freedesktop.Sdk.Extension.rust-stable"]}
        with patch.object(artifacts, "capture", side_effect=capture):
            commits = artifacts.runtime_commits(manifest)
        self.assertEqual(commits[rust], "correct-commit")
        self.assertNotIn("org.freedesktop.Sdk.Extension.rust-stable", [command[-1] for command in observed])

    def test_permissions_keep_only_current_scoped_sandbox(self):
        artifacts.validate_permissions(PERMISSIONS)
        for changed in (PERMISSIONS.replace("devices=dri;", "devices=all;"),
                        PERMISSIONS.replace("devices=dri;", "devices=dri;\nfilesystems=home;"),
                        PERMISSIONS + "org.freedesktop.secrets=talk\n",
                        PERMISSIONS + "[System Bus Policy]\n*=talk\n",
                        PERMISSIONS + "[Environment]\nWEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1\n"):
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                artifacts.validate_permissions(changed)

    def test_installed_license_counts_do_not_hide_changed_or_extra_notice_bytes(self):
        location = self.root / "installed"
        licenses = location / f"files/share/licenses/{artifacts.APP_ID}"
        licenses.mkdir(parents=True)
        shutil.copy2(self.stage / "LICENSE", licenses / "LICENSE")
        shutil.copy2(self.stage / "docs/LICENSING.md", licenses / "LICENSING.md")
        shutil.copytree(self.stage / "third-party-notices", licenses / "third-party")
        for folder in (self.stage / "packaging/third-party-licenses").iterdir():
            shutil.copytree(folder, licenses / folder.name)
        self.assertEqual(artifacts.installed_licenses(self.stage, location, VERSION)["crate_notice_directories"], 1)
        for name in ("example-1.0/LICENSE", "DEPENDENCY_LICENSES.md"):
            with self.subTest(name=name):
                target = licenses / "third-party" / name
                target.write_bytes(b"unreviewed changed notice")
                with self.assertRaisesRegex(ValueError, "notice bytes"):
                    artifacts.installed_licenses(self.stage, location, VERSION)
                shutil.copy2(self.stage / "third-party-notices" / name, target)
        (licenses / "third-party/private-extra").write_bytes(b"unexpected data")
        with self.assertRaisesRegex(ValueError, "notice bytes"):
            artifacts.installed_licenses(self.stage, location, VERSION)

    def test_installed_upstream_assets_require_exact_original_notices(self):
        location = self.root / "installed"
        licenses = location / f"files/share/licenses/{artifacts.APP_ID}"
        licenses.mkdir(parents=True)
        shutil.copy2(self.stage / "LICENSE", licenses / "LICENSE")
        shutil.copy2(self.stage / "docs/LICENSING.md", licenses / "LICENSING.md")
        shutil.copytree(self.stage / "third-party-notices", licenses / "third-party")
        for folder in (self.stage / "packaging/third-party-licenses").iterdir():
            shutil.copytree(folder, licenses / folder.name)
        proof = artifacts.installed_licenses(self.stage, location, VERSION)
        self.assertEqual(proof["upstream_notice_directories"], ["cassette-auth", "fonts", "tabler-icons"])
        for relative in ("cassette-auth/PROVENANCE.md", "fonts/Montserrat-OFL.txt",
                         "fonts/Inter-OFL.txt", "tabler-icons/LICENSE"):
            target = licenses / relative
            original = target.read_bytes()
            with self.subTest(relative=relative):
                target.write_bytes(b"changed or truncated notice")
                with self.assertRaisesRegex(ValueError, "upstream notice bytes"):
                    artifacts.installed_licenses(self.stage, location, VERSION)
                target.write_bytes(original)
        (licenses / "fonts/private-extra").write_bytes(b"unexpected extra notice")
        with self.assertRaisesRegex(ValueError, "upstream notice bytes"):
            artifacts.installed_licenses(self.stage, location, VERSION)

    def test_checksums_cover_exact_explicit_artifacts_and_reject_self_or_duplicates(self):
        self.output.mkdir()
        first, second, private = (self.output / name for name in ("source.tar.gz", "candidate.flatpak", ".private"))
        for path in (first, second, private):
            path.write_bytes(path.name.encode())
        result = self.output / "SHA256SUMS"
        artifacts.checksums(result, [first, second])
        self.assertEqual(len(result.read_text().splitlines()), 2)
        self.assertNotIn(".private", result.read_text())
        self.assertIn(f"{artifacts.sha256(first)}  {first.name}", result.read_text())
        with self.assertRaises(ValueError):
            artifacts.checksums(result, [first, first])
        with self.assertRaises(ValueError):
            artifacts.checksums(result, [result])


if __name__ == "__main__":
    unittest.main()
