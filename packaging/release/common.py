# SPDX-License-Identifier: GPL-3.0-or-later
"""Public identity and source-integrity contracts shared by release tools."""

import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib

REPOSITORY = Path(__file__).resolve().parents[2]
APP_ID = "io.github.tracerxbrhd.Karin"
BUILD_FEATURES = "cassette-auth"


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def git(repository: Path, *args: str) -> bytes:
    return subprocess.check_output(["git", "-C", str(repository), *args])


def committed_files(repository: Path, revision: str) -> dict:
    entries = []
    for row in git(repository, "ls-tree", "-r", "-z", revision).split(b"\0"):
        if not row:
            continue
        metadata, name = row.split(b"\t", 1)
        mode, kind, blob = metadata.decode().split()
        if kind != "blob" or mode not in {"100644", "100755"}:
            raise ValueError("Release source must not contain links or submodules")
        path = Path(name.decode())
        if path.is_absolute() or ".." in path.parts or path.parts[0] in {".git", "build", "target"}:
            raise ValueError("Unsafe committed source path")
        entries.append((str(path), mode == "100755", blob))
    payload = subprocess.check_output(["git", "-C", str(repository), "cat-file", "--batch"],
                                     input=("\n".join(row[2] for row in entries) + "\n").encode())
    stream = io.BytesIO(payload)
    result = {}
    for name, executable, blob in entries:
        actual, kind, size = stream.readline().decode().split()
        if actual != blob or kind != "blob":
            raise ValueError("Cannot read immutable source blob")
        result[name] = (stream.read(int(size)), executable)
        if stream.read(1) != b"\n":
            raise ValueError("Invalid Git object response")
    return result


def matches_files(root: Path, expected: dict) -> bool:
    for name, (content, executable) in expected.items():
        path = root / name
        if any(parent.is_symlink() for parent in [path, *path.parents] if parent != root.parent):
            return False
        if not path.is_file() or path.read_bytes() != content or bool(path.stat().st_mode & 0o111) != executable:
            return False
    return True


def identity(repository: Path, revision: str) -> dict:
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("Use a full lowercase Git commit, never a branch/tag")
    if git(repository, "rev-parse", "HEAD").decode().strip() != revision:
        raise ValueError("Release source must be the checked-out commit")
    if git(repository, "status", "--porcelain=v1", "--untracked-files=normal").strip():
        raise ValueError("Commit the reviewed source before preparing a release")
    source = committed_files(repository, revision)
    if set(tracked_files(repository)) != set(source) or not matches_files(repository, source):
        raise ValueError("Working bytes/modes differ from the immutable release commit")
    package = tomllib.loads(source["Cargo.toml"][0].decode())["package"]
    lock = tomllib.loads(source["Cargo.lock"][0].decode())
    own = [p for p in lock["package"] if p["name"] == "karin" and "source" not in p]
    if len(own) != 1 or own[0]["version"] != package["version"]:
        raise ValueError("Cargo.toml and Cargo.lock versions differ")
    return {"version": package["version"], "revision": revision, "source_status": "clean",
            "source_date_epoch": int(git(repository, "show", "-s", "--format=%ct", revision))}


def tracked_files(repository: Path) -> list[str]:
    return [name for name in git(repository, "ls-files", "-z").decode().split("\0") if name]


def copy_tracked(repository: Path, destination: Path) -> None:
    """Copy current tracked working bytes, never ignored/untracked private data."""
    for name in tracked_files(repository):
        relative = Path(name)
        if relative.is_absolute() or ".." in relative.parts or relative.parts[0] in {".git", "build", "target"}:
            raise ValueError(f"Unsafe tracked source path: {name}")
        source = repository / relative
        if any(path.is_symlink() for path in [source, *source.parents]):
            raise ValueError(f"Release source must not follow a symlink: {name}")
        if not source.exists():  # A development snapshot can contain deletions.
            continue
        if not source.is_file():
            raise ValueError(f"Unsupported tracked source entry: {name}")
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def locked_crates(lock: dict) -> list[dict]:
    crates = []
    for package in lock["package"]:
        if "source" not in package:
            if package["name"] != "karin":
                raise ValueError("Unreviewed local/path dependency in release lockfile")
            continue
        if package["source"] != "registry+https://github.com/rust-lang/crates.io-index":
            raise ValueError("Release generator supports the reviewed crates.io registry only")
        if not re.fullmatch(r"[a-zA-Z0-9_-]+", package["name"]) or not re.fullmatch(r"[a-zA-Z0-9.+_-]+", package["version"]):
            raise ValueError("Invalid locked crate name/version")
        if not re.fullmatch(r"[0-9a-f]{64}", package.get("checksum", "")):
            raise ValueError("Every registry crate needs its Cargo.lock SHA256")
        crates.append(package)
    return sorted(crates, key=lambda p: (p["name"], p["version"]))


def original_crate_files(package: dict) -> dict:
    """Authenticate source against Cargo.lock's complete original .crate hash."""
    name = f"{package['name']}-{package['version']}"
    cache = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")) / "registry/cache"
    archives = [p for p in cache.glob(f"*/{name}.crate") if sha256(p) == package["checksum"]]
    if not archives:
        raise ValueError(f"Missing checksum-verified {name}.crate; run cargo fetch --locked before release preparation")
    files = {}
    with tarfile.open(archives[0], "r:gz") as archive:
        for member in archive:
            path = Path(member.name)
            if path.is_absolute() or ".." in path.parts or not path.parts or path.parts[0] != name:
                raise ValueError("Unsafe original crate archive path")
            if member.isdir():
                continue
            if not member.isfile() or len(path.parts) < 2:
                raise ValueError("Unsupported original crate archive entry")
            relative = str(Path(*path.parts[1:]))
            if relative in files or relative == ".cargo-checksum.json":
                raise ValueError("Unexpected original crate checksum/duplicate file")
            # Cargo vendor omits these VCS metadata files. Build/source/license
            # files still have to match the authenticated archive byte-for-byte.
            if path.name not in {".gitignore", ".gitattributes"}:
                files[relative] = archive.extractfile(member).read()
    return files


def validate_tree(stage: Path, allowed_files: set[str]) -> None:
    allowed_dirs = set()
    for name in allowed_files:
        allowed_dirs.update(str(p) for p in Path(name).parents if str(p) != ".")
    if stage.is_symlink():
        raise ValueError("Prepared source must not be a symlink")
    for path in stage.rglob("*"):
        name = str(path.relative_to(stage))
        if path.is_symlink() or (path.is_file() and name not in allowed_files) or (path.is_dir() and name not in allowed_dirs):
            raise ValueError(f"Unexpected prepared source path: {name}")


def validate_prepared(repository: Path, stage: Path, revision: str) -> None:
    identity(repository, revision)
    if any(path.is_symlink() for path in [stage, *stage.parents]):
        raise ValueError("Prepared source must not follow an ancestor symlink")
    # The inventory subprocess runs inside the source directory; passing a
    # relative script path would otherwise prepend that directory twice.
    stage = stage.resolve(strict=True)
    if any(path.is_symlink() for path in [stage, *stage.rglob("*")]):
        raise ValueError("Prepared source must not contain or follow symlinks")
    if (stage / "BUILD-REVISION").read_text() != revision + "\n" or (stage / "BUILD-SOURCE-STATUS").read_text() != "clean\n":
        raise ValueError("Prepared source markers do not match the clean release commit")
    committed = committed_files(repository, revision)
    if not matches_files(stage, committed):
        raise ValueError("Prepared source bytes/modes differ from the immutable commit")
    allowed = set(committed) | {"BUILD-REVISION", "BUILD-SOURCE-STATUS", ".cargo/config.toml"}
    crates = locked_crates(tomllib.loads((stage / "Cargo.lock").read_text()))
    expected = {f"{p['name']}-{p['version']}" for p in crates}
    vendor = stage / "vendor"
    if {p.name for p in vendor.iterdir() if p.is_dir()} != expected:
        raise ValueError("Vendored crate set differs from Cargo.lock")
    for package in crates:
        directory = vendor / f"{package['name']}-{package['version']}"
        checksums = json.loads((directory / ".cargo-checksum.json").read_text())
        if checksums["package"] != package["checksum"]:
            raise ValueError("Vendored package checksum differs from Cargo.lock")
        original = original_crate_files(package)
        if set(checksums["files"]) != set(original):
            raise ValueError("Vendor file set differs from the checksum-verified original crate")
        files = {str(p.relative_to(directory)) for p in directory.rglob("*") if p.is_file()}
        if files != set(checksums["files"]) | {".cargo-checksum.json"} or "Cargo.toml" not in checksums["files"]:
            raise ValueError("Unexpected or incomplete files in vendored source")
        for name, value in checksums["files"].items():
            path = directory / name
            if Path(name).is_absolute() or ".." in Path(name).parts or path.is_symlink() or sha256(path) != value or path.read_bytes() != original[name]:
                raise ValueError(f"Changed vendored source: {package['name']}/{name}")
        allowed.update(f"vendor/{directory.name}/{name}" for name in files)
    notices = stage / "third-party-notices"
    if {p.name for p in notices.iterdir() if p.is_dir()} != expected or not (notices / "DEPENDENCY_LICENSES.md").is_file():
        raise ValueError("Complete reviewed crate notices are required")
    for path in notices.rglob("*"):
        if path.is_file() and (path.is_symlink() or not path.name.upper().startswith((
            "LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT", "AUTHORS", "UPSTREAM_LICENSE_SOURCE", "DEPENDENCY_LICENSES"
        ))):
            raise ValueError("Unexpected non-notice file in the source notice tree")
    config = tomllib.loads((stage / ".cargo/config.toml").read_text())
    expected_config = {"source": {"crates-io": {"replace-with": "vendored-sources"},
                                  "vendored-sources": {"directory": "vendor"}}}
    if config.get("net") == {"offline": True}:
        expected_config["net"] = {"offline": True}
    if config != expected_config:
        raise ValueError("Prepared source does not use its locked local vendor directory")
    canonical = '[source.crates-io]\nreplace-with = "vendored-sources"\n\n[source.vendored-sources]\ndirectory = "vendor"'
    if "net" in config:
        canonical += '\n\n[net]\noffline = true'
    if (stage / ".cargo/config.toml").read_text().strip() != canonical:
        raise ValueError("Prepared Cargo config must contain only the generated public vendor config")
    # Regenerate from authenticated crate sources and the committed reviewed
    # fallback notices; directory counts alone cannot prove license completeness.
    with tempfile.TemporaryDirectory(prefix="karin-release-notices-") as temporary:
        target = Path(temporary)
        subprocess.run([sys.executable, str(stage / "packaging/license_inventory.py"),
                        "--output", str(target / "inventory.md"), "--notices-dir", str(target / "notices")],
                       cwd=stage, env={**os.environ, "CARGO_NET_OFFLINE": "true"}, check=True)
        expected_notices = {str(p.relative_to(target / "notices")): p.read_bytes()
                            for p in (target / "notices").rglob("*") if p.is_file()}
        actual_notices = {str(p.relative_to(notices)): p.read_bytes() for p in notices.rglob("*") if p.is_file()}
        if actual_notices != expected_notices or (stage / "docs/DEPENDENCY_LICENSES.md").read_bytes() != (target / "inventory.md").read_bytes():
            raise ValueError("Prepared license inventory/notices differ from regenerated reviewed notices")
        allowed.update("third-party-notices/" + name for name in expected_notices)
    validate_tree(stage, allowed)


def validate_version_output(raw: str, metadata: dict, package: str) -> str:
    lines = raw.splitlines()
    required = {f"Karin {metadata['version']}", f"Source commit: {metadata['revision']}",
                "Source state: clean", f"Package: {package}", "Profile: release",
                "Features: auth-webkit,cassette-auth"}
    if not required.issubset(lines):
        raise ValueError("Artifact version/source/package/profile does not match the release")
    if not any(line.startswith("Compiler: rustc ") for line in lines) or not any(line.startswith("Target: ") and len(line) > 8 for line in lines):
        raise ValueError("The artifact must report its actual compiler and target")
    return raw


def verify_binary(binary: Path, metadata: dict, package: str) -> str:
    return validate_version_output(subprocess.check_output([str(binary.resolve()), "--version"], text=True), metadata, package)
