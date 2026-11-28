# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline regressions for release pins, source admission and metadata agreement."""

import copy
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import tomllib
import unittest
from unittest.mock import patch
import xml.etree.ElementTree as ET

from common import BUILD_FEATURES, REPOSITORY, copy_tracked, identity, validate_prepared, validate_version_output
from manifest import DEVELOPMENT, PERMISSIONS, make_manifest, verify_archive


def run(repo, *args):
    return subprocess.check_output(["git", "-C", str(repo), *args])


class ReleaseTests(unittest.TestCase):
    def test_version_metadata_and_development_install_agree(self):
        package = tomllib.loads((REPOSITORY / "Cargo.toml").read_text())["package"]
        lock = tomllib.loads((REPOSITORY / "Cargo.lock").read_text())
        own = next(p for p in lock["package"] if p["name"] == "karin" and "source" not in p)
        metainfo = ET.parse(REPOSITORY / "packaging/io.github.tracerxbrhd.Karin.metainfo.xml").getroot()
        self.assertEqual(package["version"], own["version"])
        self.assertEqual(package["version"], metainfo.find("releases/release").get("version"))
        self.assertEqual(package["license"], metainfo.findtext("project_license"))
        self.assertEqual(metainfo.findtext("id"), "io.github.tracerxbrhd.Karin")
        manifest = json.loads((REPOSITORY / DEVELOPMENT).read_text())
        self.assertEqual(manifest["finish-args"], PERMISSIONS)
        self.assertIn(
            f"cargo build --release --offline --locked --no-default-features --features {BUILD_FEATURES}",
            manifest["modules"][0]["build-commands"],
        )
        self.assertTrue(any("share/metainfo/io.github.tracerxbrhd.Karin.metainfo.xml" in c
                            for c in manifest["modules"][0]["build-commands"]))
        license_commands = [c for c in manifest["modules"][0]["build-commands"] if "/share/licenses/" in c]
        self.assertEqual(len(license_commands), 6)
        self.assertTrue(all("/app/share/licenses/io.github.tracerxbrhd.Karin/" in c for c in license_commands))
        self.assertTrue(any("third-party-notices/." in c for c in license_commands))
        self.assertIn("cp -a packaging/third-party-licenses/. /app/share/licenses/io.github.tracerxbrhd.Karin/", license_commands)
        self.assertIn("python3 packaging/release/rust_runtime.py --binary target/release/karin --output /app/share/licenses/io.github.tracerxbrhd.Karin/rust-runtime", license_commands)

    def test_all_locked_downloads_are_pinned_and_cargo_is_offline(self):
        base = json.loads((REPOSITORY / DEVELOPMENT).read_text())
        lock = tomllib.loads((REPOSITORY / "Cargo.lock").read_text())
        revision, checksum = "a" * 40, "b" * 64
        manifest = make_manifest(base, lock, revision, checksum)
        sources = manifest["modules"][0]["sources"]
        downloads = [p for p in sources if p["type"] == "archive"]
        expected = {p["checksum"] for p in lock["package"] if "source" in p}
        self.assertEqual({p["sha256"] for p in downloads[1:]}, expected)
        self.assertEqual(len(downloads) - 1, sum("source" in p for p in lock["package"]))
        self.assertTrue(all(p["archive-type"] == "tar-gzip" for p in downloads[1:]))
        self.assertEqual(downloads[0]["sha256"], checksum)
        self.assertIn(revision, downloads[0]["url"])
        self.assertFalse(any(p["type"] in ("dir", "git") for p in sources))
        self.assertEqual(manifest["build-options"]["env"]["CARGO_NET_OFFLINE"], "true")
        self.assertEqual(manifest["finish-args"], base["finish-args"])
        self.assertEqual(sources[-2]["contents"], revision + "\n")
        self.assertEqual(sources[-1]["contents"], "clean\n")
        broken = copy.deepcopy(lock)
        next(p for p in broken["package"] if "source" in p)["checksum"] = "missing"
        with self.assertRaises(ValueError):
            make_manifest(base, broken, revision, checksum)
        with self.assertRaises(ValueError):
            make_manifest(base, lock, "master", checksum)

    def test_pinned_manifest_keeps_layer_shell_in_flatpak_library_directory(self):
        base = json.loads((REPOSITORY / DEVELOPMENT).read_text())
        lock = tomllib.loads((REPOSITORY / "Cargo.lock").read_text())
        pinned = make_manifest(base, lock, "a" * 40, "b" * 64)
        development_dependencies = base["modules"][0]["modules"]
        release_dependencies = pinned["modules"][0]["modules"]
        self.assertEqual(release_dependencies, development_dependencies)
        layer_shell = next(module for module in release_dependencies if module["name"] == "gtk4-layer-shell")
        self.assertIn("--libdir=lib", layer_shell["config-opts"])
        self.assertIn("/lib/pkgconfig", layer_shell["cleanup"])
        self.assertIn("/lib/liblayer-shell-preload.so", layer_shell["cleanup"])

    def test_source_snapshot_never_copies_ignored_private_data(self):
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary) / "repo"
            repo.mkdir()
            run(repo, "init", "-q")
            (repo / ".gitignore").write_text("private-profile/\n")
            (repo / "public.rs").write_text("// Original release fixture\n")
            run(repo, "add", ".")
            (repo / "private-profile").mkdir()
            (repo / "private-profile/session.json").write_text("original synthetic private sentinel")
            (repo / "untracked.txt").write_text("untracked fixture")
            output = Path(temporary) / "snapshot"
            output.mkdir()
            copy_tracked(repo, output)
            self.assertEqual({str(p.relative_to(output)) for p in output.rglob("*") if p.is_file()}, {".gitignore", "public.rs"})
            (repo / "tracked-link").symlink_to(repo / "private-profile/session.json")
            run(repo, "add", "tracked-link")
            with self.assertRaises(ValueError):
                copy_tracked(repo, output)

    def test_archive_rejects_extra_files_wrong_bytes_and_executable_mode(self):
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            run(repo, "init", "-q")
            (repo / "source.rs").write_text("// Original source\n")
            run(repo, "add", ".")
            run(repo, "-c", "user.name=Release fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "source")
            revision = run(repo, "rev-parse", "HEAD").decode().strip()

            def archive(extra=False, content=b"// Original source\n", mode=0o644):
                output = io.BytesIO()
                with tarfile.open(fileobj=output, mode="w:gz") as tar:
                    member = tarfile.TarInfo("upstream/source.rs")
                    member.size, member.mode = len(content), mode
                    tar.addfile(member, io.BytesIO(content))
                    if extra:
                        member = tarfile.TarInfo("upstream/private.json")
                        member.size = 1
                        tar.addfile(member, io.BytesIO(b"x"))
                return output.getvalue()

            verify_archive(repo, revision, archive())
            for payload in (archive(extra=True), archive(content=b"modified"), archive(mode=0o755)):
                with self.assertRaises(ValueError):
                    verify_archive(repo, revision, payload)

    def test_release_rejects_wrong_version_dirty_or_experimental_binary(self):
        metadata = {"version": "0.8.0", "revision": "a" * 40}
        features = "auth-webkit,browser-session-auth"
        version = (
            f"Karin 0.8.0\nSource commit: {'a' * 40}\nSource state: clean\nPackage: Native\n"
            f"Profile: release\nCompiler: rustc 1.98.1\nTarget: x86_64-unknown-linux-gnu\nFeatures: {features}\n"
        )
        validate_version_output(version, metadata, "Native")
        for old, new in (("0.8.0", "0.0.0"), ("state: clean", "state: modified"),
                         ("Package: Native", "Package: Flatpak"), ("Profile: release", "Profile: debug"),
                         (features, "auth-webkit,cassette-auth"),
                         (features, f"{features},experimental-device-auth")):
            with self.assertRaises(ValueError):
                validate_version_output(version.replace(old, new), metadata, "Native")

    def test_git_index_flags_cannot_hide_changed_release_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            run(repo, "init", "-q")
            (repo / "Cargo.toml").write_text('[package]\nname="karin"\nversion="0.8.0"\n')
            (repo / "Cargo.lock").write_text('version=4\n[[package]]\nname="karin"\nversion="0.8.0"\n')
            run(repo, "add", ".")
            run(repo, "-c", "user.name=Release fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "source")
            revision = run(repo, "rev-parse", "HEAD").decode().strip()
            self.assertEqual(identity(repo, revision)["version"], "0.8.0")
            run(repo, "update-index", "--assume-unchanged", "Cargo.toml")
            (repo / "Cargo.toml").write_text('[package]\nname="karin"\nversion="unreviewed"\n')
            self.assertFalse(run(repo, "status", "--porcelain").strip())
            with self.assertRaises(ValueError):
                identity(repo, revision)

    def test_normal_python_import_keeps_clean_checkout_clean(self):
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            run(repo, "init", "-q")
            (repo / "Cargo.toml").write_text('[package]\nname="karin"\nversion="0.8.0"\n')
            (repo / "Cargo.lock").write_text('version=4\n[[package]]\nname="karin"\nversion="0.8.0"\n')
            (repo / ".gitignore").write_bytes((REPOSITORY / ".gitignore").read_bytes())
            (repo / "common.py").write_bytes((REPOSITORY / "packaging/release/common.py").read_bytes())
            run(repo, "add", ".")
            run(repo, "-c", "user.name=Release fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "source")
            revision = run(repo, "rev-parse", "HEAD").decode().strip()
            environment = dict(os.environ)
            environment.pop("PYTHONDONTWRITEBYTECODE", None)
            subprocess.run([os.sys.executable, "-c", f"from pathlib import Path; import common; common.identity(Path.cwd(), '{revision}')"],
                           cwd=repo, env=environment, check=True)
            self.assertTrue(list((repo / "__pycache__").glob("*.pyc")))
            self.assertFalse(run(repo, "status", "--porcelain").strip())

    def test_prepared_source_rejects_private_extras_rehashed_vendor_and_false_notices(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            repo = base / "repo"
            repo.mkdir()
            run(repo, "init", "-q")
            original = {"Cargo.toml": b'[package]\nname="fixture"\nversion="1.0.0"\n',
                        "src/lib.rs": b"// Original source fixture\n", "LICENSE": b"Original license fixture\n"}
            archive = base / "cargo/registry/cache/original/fixture-1.0.0.crate"
            archive.parent.mkdir(parents=True)
            with tarfile.open(archive, "w:gz") as tar:
                for name, content in original.items():
                    entry = tarfile.TarInfo("fixture-1.0.0/" + name)
                    entry.size = len(content)
                    tar.addfile(entry, io.BytesIO(content))
                entry = tarfile.TarInfo("fixture-1.0.0/.gitignore")
                entry.size = len(b"target/\n")
                tar.addfile(entry, io.BytesIO(b"target/\n"))
            checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
            (repo / "Cargo.toml").write_text('[package]\nname="karin"\nversion="0.8.0"\n')
            (repo / "Cargo.lock").write_text('version=4\n[[package]]\nname="karin"\nversion="0.8.0"\n[[package]]\nname="fixture"\nversion="1.0.0"\nsource="registry+https://github.com/rust-lang/crates.io-index"\nchecksum="' + checksum + '"\n')
            (repo / "docs").mkdir()
            (repo / "docs/DEPENDENCY_LICENSES.md").write_text("Original inventory fixture\n")
            (repo / "packaging").mkdir()
            (repo / "packaging/license_inventory.py").write_text('''import argparse
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output');p.add_argument('--notices-dir');a=p.parse_args()
text='Original inventory fixture\\n';Path(a.output).write_text(text)
n=Path(a.notices_dir);(n/'fixture-1.0.0').mkdir(parents=True)
(n/'DEPENDENCY_LICENSES.md').write_text(text)
(n/'fixture-1.0.0/LICENSE').write_bytes(Path('vendor/fixture-1.0.0/LICENSE').read_bytes())
''')
            run(repo, "add", ".")
            run(repo, "-c", "user.name=Release fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "source")
            revision = run(repo, "rev-parse", "HEAD").decode().strip()
            stage = base / "stage"
            stage.mkdir()
            copy_tracked(repo, stage)
            (stage / "BUILD-REVISION").write_text(revision + "\n")
            (stage / "BUILD-SOURCE-STATUS").write_text("clean\n")
            (stage / ".cargo").mkdir()
            (stage / ".cargo/config.toml").write_text('[source.crates-io]\nreplace-with = "vendored-sources"\n\n[source.vendored-sources]\ndirectory = "vendor"\n')
            vendor = stage / "vendor/fixture-1.0.0"
            for name, content in original.items():
                path = vendor / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(content)
            checksums = {"package": checksum, "files": {name: hashlib.sha256(content).hexdigest() for name, content in original.items()}}
            (vendor / ".cargo-checksum.json").write_text(json.dumps(checksums))
            notice = stage / "third-party-notices/fixture-1.0.0/LICENSE"
            notice.parent.mkdir(parents=True)
            notice.write_bytes(original["LICENSE"])
            (stage / "third-party-notices/DEPENDENCY_LICENSES.md").write_text("Original inventory fixture\n")
            with patch.dict(os.environ, {"CARGO_HOME": str(base / "cargo")}):
                validate_prepared(repo, stage, revision)
                previous = Path.cwd()
                try:
                    os.chdir(repo)
                    validate_prepared(repo, Path("../stage"), revision)
                finally:
                    os.chdir(previous)
                for extra in (stage / "private-profile.json", stage / ".cargo/credentials.toml"):
                    extra.write_text("Original private sentinel")
                    with self.assertRaises(ValueError):
                        validate_prepared(repo, stage, revision)
                    extra.unlink()
                notice.write_text("Incomplete or false notice")
                with self.assertRaises(ValueError):
                    validate_prepared(repo, stage, revision)
                notice.write_bytes(original["LICENSE"])
                changed = b"// Unreviewed replacement source\n"
                (vendor / "src/lib.rs").write_bytes(changed)
                checksums["files"]["src/lib.rs"] = hashlib.sha256(changed).hexdigest()
                (vendor / ".cargo-checksum.json").write_text(json.dumps(checksums))
                with self.assertRaises(ValueError):
                    validate_prepared(repo, stage, revision)


if __name__ == "__main__":
    unittest.main()
