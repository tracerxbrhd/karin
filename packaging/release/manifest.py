#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate a standalone, checksum-pinned source Flatpak for a clean commit.

This builds Karin's candidate locally; it is not a Flathub submission. Sources
are an exact verified public Git archive and Cargo.lock-pinned crate archives.
Cargo runs offline after Flatpak Builder has verified those downloads.
"""

import argparse
import copy
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import tarfile
import tomllib
import urllib.parse
import urllib.request

from common import APP_ID, REPOSITORY, git, identity, locked_crates, write_json

DEVELOPMENT = "packaging/flatpak/io.github.tracerxbrhd.Karin.json"
DISCORD_FILESYSTEMS = ("xdg-run/discord-ipc-0:ro", "xdg-run/app/com.discordapp.Discord:ro")
PERMISSIONS = ["--share=network", "--share=ipc", "--socket=wayland", "--socket=fallback-x11",
               "--socket=pulseaudio", "--device=dri", f"--own-name=org.mpris.MediaPlayer2.{APP_ID}",
               *(f"--filesystem={path}" for path in DISCORD_FILESYSTEMS)]


def source_url(revision: str) -> str:
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("A full immutable commit is required")
    return f"https://github.com/tracerxbrhd/karin/archive/{revision}.tar.gz"


def make_manifest(base: dict, lock: dict, revision: str, source_sha256: str) -> dict:
    source_url(revision)
    if not re.fullmatch(r"[0-9a-f]{64}", source_sha256):
        raise ValueError("A verified source-archive SHA256 is required")
    if base["app-id"] != APP_ID or base["runtime-version"] != "50" or base["finish-args"] != PERMISSIONS:
        raise ValueError("The reviewed application identity/runtime/permissions changed")
    if len(base["modules"]) != 1 or base["modules"][0]["name"] != "karin":
        raise ValueError("Review new build modules before preparing a release")
    result = copy.deepcopy(base)
    sources = [{"type": "archive", "url": source_url(revision), "sha256": source_sha256,
                "strip-components": 1}]
    for package in locked_crates(lock):
        name, version = package["name"], package["version"]
        directory = f"vendor/{name}-{version}"
        filename = urllib.parse.quote(f"{name}-{version}.crate", safe="-._")
        sources.extend([
            {"type": "archive", "archive-type": "tar-gzip", "url": f"https://static.crates.io/crates/{name}/{filename}",
             "sha256": package["checksum"], "dest": directory, "strip-components": 1},
            {"type": "inline", "dest": directory, "dest-filename": ".cargo-checksum.json",
             "contents": json.dumps({"package": package["checksum"], "files": {}}, sort_keys=True)},
        ])
    sources.extend([
        {"type": "inline", "dest": ".cargo", "dest-filename": "config.toml",
         "contents": '[source.crates-io]\nreplace-with = "vendored-sources"\n\n[source.vendored-sources]\ndirectory = "vendor"\n\n[net]\noffline = true\n'},
        {"type": "inline", "dest-filename": "BUILD-REVISION", "contents": revision + "\n"},
        {"type": "inline", "dest-filename": "BUILD-SOURCE-STATUS", "contents": "clean\n"},
    ])
    module = result["modules"][0]
    module["sources"] = sources
    module["build-commands"].insert(0, "python3 packaging/license_inventory.py --output docs/DEPENDENCY_LICENSES.md --notices-dir third-party-notices")
    result["build-options"]["env"]["CARGO_NET_OFFLINE"] = "true"
    return result


def verify_archive(repository: Path, revision: str, payload: bytes) -> None:
    """Match every downloaded source file/mode to the public Git object tree."""
    expected = {}
    with tarfile.open(fileobj=io.BytesIO(git(repository, "archive", "--format=tar", revision))) as archive:
        for member in archive:
            if member.isfile():
                expected[member.name] = (archive.extractfile(member).read(), bool(member.mode & 0o111))
            elif not member.isdir():
                raise ValueError("Unsupported link/special entry in the Git source")
    tracked = {name for name in git(repository, "ls-tree", "-r", "--name-only", "-z", revision).decode().split("\0") if name}
    if set(expected) != tracked:
        raise ValueError("Git export omitted tracked corresponding source")
    found = {}
    roots = set()
    with tarfile.open(fileobj=io.BytesIO(payload), mode="r:gz") as archive:
        for member in archive:
            path = PurePosixPath(member.name)
            if path.is_absolute() or ".." in path.parts:
                raise ValueError("Unsafe source archive path")
            if member.isdir():
                continue
            if not member.isfile() or len(path.parts) < 2:
                raise ValueError("Unsupported source archive entry")
            roots.add(path.parts[0])
            name = str(PurePosixPath(*path.parts[1:]))
            if name in found or name not in expected or member.size != len(expected[name][0]) or member.mode & 0o7000:
                raise ValueError("Unexpected, duplicate or unsafe source archive entry")
            found[name] = (archive.extractfile(member).read(), bool(member.mode & 0o111))
    if len(roots) != 1 or found != expected:
        raise ValueError("Downloaded source bytes/modes differ from the exact Git commit")


def provenance(manifest: dict, metadata: dict, repository: Path = REPOSITORY) -> dict:
    revision = metadata["revision"]
    first = manifest["modules"][0]["sources"][0]
    lock_bytes = git(repository, "show", f"{revision}:Cargo.lock")
    base = json.loads(git(repository, "show", f"{revision}:{DEVELOPMENT}"))
    expected = make_manifest(base, tomllib.loads(lock_bytes.decode()), revision, first["sha256"])
    if manifest != expected:
        raise ValueError("Release manifest differs from exact-source/lock/permission generation")
    return {**metadata, "source_url": first["url"], "source_sha256": first["sha256"],
            "cargo_lock_sha256": hashlib.sha256(lock_bytes).hexdigest()}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    metadata = identity(REPOSITORY, args.revision)
    if args.output.exists():
        raise SystemExit("Use a fresh manifest output path")
    with urllib.request.urlopen(source_url(args.revision), timeout=30) as response:
        payload = response.read(64 * 1024 * 1024 + 1)
    if len(payload) > 64 * 1024 * 1024:
        raise SystemExit("Unexpectedly large public source archive")
    verify_archive(REPOSITORY, args.revision, payload)
    base = json.loads(git(REPOSITORY, "show", f"{args.revision}:{DEVELOPMENT}"))
    lock = tomllib.loads(git(REPOSITORY, "show", f"{args.revision}:Cargo.lock").decode())
    manifest = make_manifest(base, lock, args.revision, hashlib.sha256(payload).hexdigest())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    write_json(args.output, manifest)
    write_json(args.output.with_suffix(".BUILD-INFO.json"), {
        **provenance(manifest, metadata), "manifest_kind": "standalone source candidate",
        "repeatability": "Source-repeatable and checksum-verifiable; independent binary reproducibility not claimed",
        "publication": "Not a public release or Flathub submission",
    })
    print(args.output)


if __name__ == "__main__":
    main()
