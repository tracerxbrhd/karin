#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Stage the public release payload and check the bytes advertised for publication."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile

from common import sha256, write_json

SOURCE_REPOSITORY = "tracerxbrhd/karin-source"
UPDATE_RECORD = "KARIN-FLATPAK-UPDATE.json"
RECEIPT = "publication.json"
UPDATE_MARKER = "karin-flatpak-update-sha256"


def release_assets(version: str) -> set[str]:
    component = r"(0|[1-9][0-9]*)"
    if not isinstance(version, str) or not re.fullmatch(rf"{component}\.{component}\.{component}", version):
        raise ValueError("Publication requires a stable version")
    if tuple(map(int, version.split("."))) < (0, 8, 2):
        raise ValueError("The new publication contract starts at 0.8.2")
    names = {f"karin-{version}-x86_64.flatpak", f"karin-{version}-linux-x86_64.tar.gz"}
    if version == "0.8.2":
        names.add(UPDATE_RECORD)
    return names


def source_location(version: str) -> tuple[str, str]:
    name = f"karin-{version}-source.tar.gz"
    return name, f"https://github.com/{SOURCE_REPOSITORY}/releases/download/v{version}/{name}"


def file_identity(path: Path) -> dict:
    if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
        raise ValueError(f"Missing or empty publication input: {path.name}")
    return {"size": path.stat().st_size, "digest": f"sha256:{sha256(path)}"}


def notes_snippet(manifest: dict) -> str:
    return (
        f"[Соответствующие исходники]({manifest['source']['url']})\n\n"
        f"<!-- {UPDATE_MARKER}: {manifest['update_record_sha256']} -->\n"
    )


def prepare(channel: Path, version: str, revision: str) -> Path:
    names = release_assets(version)
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("Publication requires a reviewed full Git commit")
    packages = channel / "packages"
    public = channel / "release-assets"
    corresponding = channel / "corresponding-source"
    receipt = packages / RECEIPT
    notes = packages / "RELEASE-NOTES-SNIPPET.md"
    if any(path.exists() or path.is_symlink() for path in (public, corresponding, receipt, notes)):
        raise ValueError("Publication output already exists")

    record_bytes = (packages / UPDATE_RECORD).read_bytes()
    record = json.loads(record_bytes)
    if (record["version"] != version or record["source_commit"] != revision
            or record_bytes != (channel / "site" / "flatpak" / UPDATE_RECORD).read_bytes()):
        raise ValueError("The signed channel update record does not match this release")

    archive_name, url = source_location(version)
    manifest = {
        "version": version,
        "revision": revision,
        "update_record_sha256": hashlib.sha256(record_bytes).hexdigest(),
        "assets": {name: file_identity(packages / name) for name in sorted(names)},
        "source": {"name": archive_name, "url": url, **file_identity(packages / archive_name)},
    }
    public.mkdir()
    corresponding.mkdir()
    for name in sorted(names):
        (packages / name).rename(public / name)
    (packages / archive_name).rename(corresponding / archive_name)
    write_json(receipt, manifest)
    notes.write_text(notes_snippet(manifest), encoding="utf-8")
    return receipt


def load_receipt(path: Path) -> dict:
    manifest = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(manifest, dict):
        raise ValueError("Invalid publication receipt")
    names = release_assets(manifest.get("version"))
    revision = manifest.get("revision")
    record_sha = manifest.get("update_record_sha256")
    assets = manifest.get("assets")
    source = manifest.get("source")
    if (not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}", revision)
            or not isinstance(record_sha, str) or not re.fullmatch(r"[0-9a-f]{64}", record_sha)
            or not isinstance(assets, dict) or set(assets) != names or not isinstance(source, dict)):
        raise ValueError("Invalid publication receipt identity or file set")
    archive_name, url = source_location(manifest["version"])
    if source.get("name") != archive_name or source.get("url") != url:
        raise ValueError("Source archive must use the canonical versioned location")
    for item in [*assets.values(), source]:
        if (not isinstance(item, dict) or type(item.get("size")) is not int or item["size"] <= 0
                or not isinstance(item.get("digest"), str)
                or not re.fullmatch(r"sha256:[0-9a-f]{64}", item["digest"])):
            raise ValueError("Malformed publication file identity")
    if UPDATE_RECORD in assets and assets[UPDATE_RECORD]["digest"] != f"sha256:{record_sha}":
        raise ValueError("Transition JSON differs from the Pages update record")
    return manifest


def verify_source(manifest: dict) -> None:
    source = manifest["source"]
    # A private repository, draft source release, missing asset or different
    # archive fails here without using permissions from the current checkout.
    with tempfile.TemporaryDirectory(prefix="karin-public-source-") as directory:
        downloaded = Path(directory) / source["name"]
        subprocess.run([
            "curl", "--disable", "--fail", "--silent", "--show-error", "--location",
            "--proto", "=https", "--proto-redir", "=https", "--max-redirs", "4",
            "--connect-timeout", "15", "--max-time", "120", "--max-filesize", str(source["size"]),
            "--output", str(downloaded), source["url"],
        ], check=True)
        if file_identity(downloaded) != {key: source[key] for key in ("size", "digest")}:
            raise ValueError("The public corresponding source archive differs from the verified build")


def verify_release(manifest: dict, release: dict, revision: str) -> None:
    if (manifest["revision"] != revision or not isinstance(release, dict)
            or release.get("tag_name") != f"v{manifest['version']}"
            or release.get("draft") is not False or release.get("prerelease") is not False):
        raise ValueError("Published Latest release differs from the reviewed stable Git revision")
    expected = manifest["assets"].copy()
    assets = release.get("assets")
    if not isinstance(assets, list):
        raise ValueError("Missing GitHub release assets")
    for item in assets:
        name = item.get("name") if isinstance(item, dict) else None
        identity = expected.pop(name, None) if isinstance(name, str) else None
        if (identity is None or item.get("state") != "uploaded" or type(item.get("size")) is not int
                or item.get("size") != identity["size"] or item.get("digest") != identity["digest"]):
            raise ValueError("Release assets must match the exact verified public files")
    if expected:
        raise ValueError("Release assets are incomplete")
    body = release.get("body")
    source_url = manifest["source"]["url"]
    if not isinstance(body, str) or not (
        f"]({source_url})" in body or f"<{source_url}>" in body or source_url in body.split()
    ):
        raise ValueError("Release notes must link to the exact public corresponding source archive")
    markers = [
        line for line in body.replace("\r\n", "\n").split("\n")
        if re.search(UPDATE_MARKER, line, re.ASCII | re.IGNORECASE)
    ]
    if markers != [f"<!-- {UPDATE_MARKER}: {manifest['update_record_sha256']} -->"]:
        raise ValueError("Release notes must contain exactly one matching Flatpak update digest")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    stage = commands.add_parser("prepare")
    stage.add_argument("--channel", type=Path, required=True)
    stage.add_argument("--version", required=True)
    stage.add_argument("--revision", required=True)
    source = commands.add_parser("verify-source")
    source.add_argument("receipt", type=Path)
    release = commands.add_parser("verify-release")
    release.add_argument("receipt", type=Path)
    release.add_argument("release", type=Path)
    release.add_argument("--revision", required=True)
    args = parser.parse_args()
    try:
        if args.command == "prepare":
            print(prepare(args.channel, args.version, args.revision))
        else:
            manifest = load_receipt(args.receipt)
            if args.command == "verify-source":
                verify_source(manifest)
                print(notes_snippet(manifest), end="")
            else:
                verify_release(manifest, json.loads(args.release.read_text(encoding="utf-8")), args.revision)
    except (OSError, KeyError, TypeError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Stable release validation failed: {error}\n")


if __name__ == "__main__":
    main()
