# SPDX-License-Identifier: GPL-3.0-or-later

import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import publication

REVISION = "a" * 40
UPDATE = publication.UPDATE_RECORD


class PublicationTests(unittest.TestCase):
    def prepare(self, root: Path, version: str) -> tuple[dict, dict]:
        packages = root / "packages"
        pages = root / "site" / "flatpak"
        packages.mkdir()
        pages.mkdir(parents=True)
        source_name, _ = publication.source_location(version)
        update = json.dumps({"schema": 1, "version": version, "source_commit": REVISION}).encode()
        (packages / UPDATE).write_bytes(update)
        (pages / UPDATE).write_bytes(update)
        for name in publication.release_assets(version) - {UPDATE} | {source_name, "BUILD-INFO.txt"}:
            (packages / name).write_bytes(f"fixture for {name}".encode())
        receipt = publication.prepare(root, version, REVISION)
        manifest = publication.load_receipt(receipt)
        release = {
            "tag_name": f"v{version}",
            "draft": False,
            "prerelease": False,
            "assets": [
                {"name": name, "state": "uploaded", **identity}
                for name, identity in manifest["assets"].items()
            ],
            "body": publication.notes_snippet(manifest),
        }
        return manifest, release

    def test_transition_and_later_release_contain_only_expected_files(self):
        for version in ("0.8.2", "0.8.3", "1.4.0"):
            with self.subTest(version=version), tempfile.TemporaryDirectory() as work:
                root = Path(work)
                manifest, release = self.prepare(root, version)
                expected = {
                    f"karin-{version}-x86_64.flatpak",
                    f"karin-{version}-linux-x86_64.tar.gz",
                }
                if version == "0.8.2":
                    expected.add(UPDATE)
                    self.assertEqual(
                        (root / "release-assets" / UPDATE).read_bytes(),
                        (root / "site" / "flatpak" / UPDATE).read_bytes(),
                    )
                self.assertEqual({path.name for path in (root / "release-assets").iterdir()}, expected)
                self.assertEqual(
                    {path.name for path in (root / "corresponding-source").iterdir()},
                    {f"karin-{version}-source.tar.gz"},
                )
                self.assertEqual(
                    (root / "packages" / "RELEASE-NOTES-SNIPPET.md").read_text(),
                    release["body"],
                )
                self.assertTrue((root / "packages" / "BUILD-INFO.txt").is_file())
                publication.verify_release(manifest, release, REVISION)

    def test_release_guard_rejects_missing_extra_changed_or_duplicate_assets(self):
        for version in ("0.8.2", "0.8.3"):
            with self.subTest(version=version), tempfile.TemporaryDirectory() as work:
                manifest, release = self.prepare(Path(work), version)
                assets = release["assets"]
                for shape in (
                    assets[:-1],
                    assets + [{"name": "SHA256SUMS"}],
                    assets[1:] + [assets[1]],
                    [{**assets[0], "digest": "sha256:" + "0" * 64}, *assets[1:]],
                    [{**assets[0], "state": "new"}, *assets[1:]],
                    [{**assets[0], "size": assets[0]["size"] + 1}, *assets[1:]],
                ):
                    with self.assertRaises(ValueError):
                        publication.verify_release(manifest, {**release, "assets": shape}, REVISION)
                for changed_release in (
                    {**release, "tag_name": "v0.8.1"},
                    {**release, "draft": True},
                    {**release, "prerelease": True},
                ):
                    with self.assertRaises(ValueError):
                        publication.verify_release(manifest, changed_release, REVISION)
                with self.assertRaises(ValueError):
                    publication.verify_release(manifest, release, "b" * 40)

    def test_release_requires_the_versioned_source_link_and_single_canonical_marker(self):
        with tempfile.TemporaryDirectory() as work:
            manifest, release = self.prepare(Path(work), "0.8.3")
            notes = release["body"]
            link, marker = notes.strip().split("\n\n")
            publication.verify_release(manifest, {**release, "body": notes.replace("\n", "\r\n")}, REVISION)
            for body in (
                marker,
                link,
                notes + marker,
                notes + marker.upper(),
                notes.replace(manifest["update_record_sha256"], "0" * 64),
                notes.replace(marker, "  " + marker),
                link + "\n" + marker + "\r",
                notes.replace("releases/download/v0.8.3/", "releases/latest/download/"),
            ):
                with self.subTest(body=body), self.assertRaises(ValueError):
                    publication.verify_release(manifest, {**release, "body": body}, REVISION)

    def test_assembly_rejects_a_site_record_different_from_the_legacy_asset(self):
        with tempfile.TemporaryDirectory() as work:
            root = Path(work)
            packages = root / "packages"
            pages = root / "site" / "flatpak"
            packages.mkdir()
            pages.mkdir(parents=True)
            update = json.dumps({"schema": 1, "version": "0.8.2", "source_commit": REVISION})
            (packages / UPDATE).write_text(update)
            (pages / UPDATE).write_text(update + "\n")
            with self.assertRaisesRegex(ValueError, "update record"):
                publication.prepare(root, "0.8.2", REVISION)
            self.assertFalse((root / "release-assets").exists())

    def test_receipt_rejects_an_external_archive_and_changed_legacy_digest(self):
        with tempfile.TemporaryDirectory() as work:
            root = Path(work)
            manifest, _ = self.prepare(root, "0.8.2")
            receipt = root / "packages" / publication.RECEIPT
            for changed in (
                {**manifest, "source": {**manifest["source"], "url": "https://example.com/source.tar.gz"}},
                {**manifest, "update_record_sha256": "f" * 64},
            ):
                receipt.write_text(json.dumps(changed))
                with self.assertRaises(ValueError):
                    publication.load_receipt(receipt)

    def test_corresponding_source_requires_an_anonymous_download_of_exact_bytes(self):
        with tempfile.TemporaryDirectory() as work:
            root = Path(work)
            manifest, _ = self.prepare(root, "0.8.3")
            source = manifest["source"]
            payload = (root / "corresponding-source" / source["name"]).read_bytes()
            self.assertEqual(source["digest"], f"sha256:{hashlib.sha256(payload).hexdigest()}")

            def curl(command, *, check):
                self.assertTrue(check)
                self.assertEqual(command[:2], ["curl", "--disable"])
                self.assertEqual(command[-1], source["url"])
                self.assertNotIn("--user", command)
                self.assertNotIn("--header", command)
                self.assertEqual(command[command.index("--proto-redir") + 1], "=https")
                self.assertEqual(command[command.index("--max-filesize") + 1], str(len(payload)))
                Path(command[command.index("--output") + 1]).write_bytes(download)

            with patch("publication.subprocess.run", side_effect=curl):
                download = payload
                publication.verify_source(manifest)
                download = b"0" * len(payload)
                with self.assertRaisesRegex(ValueError, "differs"):
                    publication.verify_source(manifest)
            with patch("publication.subprocess.run", side_effect=subprocess.CalledProcessError(22, "curl")):
                with self.assertRaises(subprocess.CalledProcessError):
                    publication.verify_source(manifest)
