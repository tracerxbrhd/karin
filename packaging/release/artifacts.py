#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Build local, source-repeatable release artifacts; never publish a release."""

import argparse
import configparser
import gzip
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import platform
import re
import subprocess
import tarfile
import tomllib

from common import (APP_ID, BUILD_FEATURES, REPOSITORY, identity, locked_crates, sha256,
                    committed_files, validate_prepared, validate_version_output,
                    verify_binary, write_json)
from manifest import DISCORD_FILESYSTEMS, provenance
from rust_runtime import binary_identity, runtime_entries, validate_runtime

APP_ICON_SIZES = (16, 24, 32, 48, 64, 128, 256)


def capture(command: list[str]) -> str:
    return subprocess.check_output(command, text=True, stderr=subprocess.STDOUT).strip()


def output_directory(path: Path) -> Path:
    path = path.resolve()
    path.mkdir(parents=True, exist_ok=True)
    return path


def artifact_name(metadata: dict) -> str:
    version = metadata["version"]
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9.+_-]*", version):
        raise ValueError("Unsafe artifact version")
    return f"karin-{version}"


def build_info(metadata: dict, kind: str) -> dict:
    return {**metadata, "artifact_kind": kind, "app_id": APP_ID,
            "rebuild_command": f"cargo build --release --offline --locked --no-default-features --features {BUILD_FEATURES}",
            "repeatability": "Source-repeatable and checksum-verifiable; independent binary reproducibility not claimed",
            "publication": "Prepared artifacts; no public tag, published release or Flathub submission performed"}


def tree_files(directory: Path) -> list[Path]:
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError(f"Expected a regular source directory: {directory}")
    result = []
    for path in sorted(directory.rglob("*")):
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ValueError(f"Unsupported source link/special entry: {path}")
        if path.is_file():
            result.append(path)
    return result


def source_entries(repository: Path, stage: Path, revision: str) -> dict[str, Path]:
    """Archive only tracked project files and the required generated sources."""
    names = set(committed_files(repository, revision)) | {"BUILD-REVISION", "BUILD-SOURCE-STATUS", ".cargo/config.toml"}
    for package in sorted((stage / "vendor").iterdir()):
        if package.is_symlink() or not package.is_dir():
            raise ValueError("Unexpected vendored entry")
        marker = package / ".cargo-checksum.json"
        files = json.loads(marker.read_text())["files"]
        if "Cargo.toml" not in files:
            raise ValueError("Prepared vendor sources need a complete Cargo file checksum inventory")
        names.add(str(marker.relative_to(stage)))
        for name in files:
            relative = PurePosixPath(name)
            if relative.is_absolute() or ".." in relative.parts:
                raise ValueError("Unsafe vendored source path")
            names.add(str((package / name).relative_to(stage)))
    names.update(str(path.relative_to(stage)) for path in tree_files(stage / "third-party-notices"))
    result = {}
    for name in sorted(names):
        relative = PurePosixPath(name)
        if relative.is_absolute() or ".." in relative.parts or relative.parts[0] in {".git", "target", "build", "cargo-home", ".aws", ".ssh"}:
            raise ValueError(f"Unsafe release source path: {name}")
        path = stage / name
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"Missing or linked release source file: {name}")
        result[name] = path
    return result


def archive(destination: Path, entries: dict[str, Path | bytes], epoch: int,
            executables: set[str] | None = None) -> None:
    if destination.exists():
        raise ValueError(f"Use a fresh artifact path: {destination}")
    temporary = destination.with_name(destination.name + ".partial")
    if temporary.exists():
        raise ValueError(f"Inspect the previous partial artifact first: {temporary}")
    try:
        with temporary.open("xb") as output, gzip.GzipFile(filename="", mode="wb", fileobj=output, mtime=epoch) as compressed:
            with tarfile.open(fileobj=compressed, mode="w") as tar:
                for name, value in sorted(entries.items()):
                    relative = PurePosixPath(name)
                    if relative.is_absolute() or ".." in relative.parts:
                        raise ValueError("Unsafe archive entry")
                    mode = 0o644
                    if isinstance(value, Path):
                        if value.is_symlink() or not value.is_file():
                            raise ValueError(f"Unsupported archive source: {value}")
                        size = value.stat().st_size
                        stream = value.open("rb")
                        if value.stat().st_mode & 0o111:
                            mode = 0o755
                    else:
                        size = len(value)
                        stream = io.BytesIO(value)
                    if name in (executables or set()):
                        mode = 0o755
                    header = tarfile.TarInfo(name)
                    header.size, header.mode, header.mtime = size, mode, epoch
                    with stream:
                        tar.addfile(header, stream)
        temporary.replace(destination)
    except BaseException:
        temporary.unlink(missing_ok=True)
        raise


def json_bytes(value: dict) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def verify_archive(path: Path, entries: dict[str, Path | bytes]) -> None:
    """Reuse a source archive only after matching all current source bytes."""
    with tarfile.open(path, "r:gz") as tar:
        members = tar.getmembers()
        if len(members) != len(entries) or {m.name for m in members} != set(entries):
            raise ValueError("Existing source archive has unexpected or missing entries")
        for member in members:
            if not member.isfile() or member.mode & 0o7000:
                raise ValueError("Existing source archive contains a link/special entry")
            expected = entries[member.name]
            executable = isinstance(expected, Path) and bool(expected.stat().st_mode & 0o111)
            if bool(member.mode & 0o111) != executable:
                raise ValueError("Existing source archive executable modes differ")
            value = sha256(expected) if isinstance(expected, Path) else hashlib.sha256(expected).hexdigest()
            if hashlib.file_digest(tar.extractfile(member), "sha256").hexdigest() != value:
                raise ValueError("Existing source archive differs from prepared source")


def source_artifact(repository: Path, revision: str, output: Path,
                    prepared: Path | None = None) -> Path:
    metadata = identity(repository, revision)
    output = output_directory(output)
    if prepared is None:
        subprocess.run(["python3", str(repository / "packaging/flatpak/prepare-source.py")], cwd=repository, check=True)
        prepared = repository / "build/flatpak-source"
    validate_prepared(repository, prepared, revision)
    name = artifact_name(metadata)
    info = build_info(metadata, "complete corresponding source with locked vendor and notices")
    entries = {f"{name}/{path}": value for path, value in source_entries(repository, prepared, revision).items()}
    entries[f"{name}/BUILD-INFO.json"] = json_bytes(info)
    target = output / f"{name}-source.tar.gz"
    if target.exists():
        verify_archive(target, entries)
    else:
        archive(target, entries, metadata["source_date_epoch"])
    identity(repository, revision)
    write_json(output / f"{name}-source.BUILD-INFO.json", info)
    return target


def native_libraries(binary: Path) -> dict:
    packages = ("gtk4", "libadwaita-1", "gstreamer-1.0", "webkitgtk-6.0")
    versions = {name: capture(["pkg-config", "--modversion", name]) for name in packages}
    linked = re.sub(r"\s*\(0x[0-9a-fA-F]+\)", "", capture(["ldd", str(binary)]))
    if "not found" in linked:
        raise ValueError("Native artifact has unresolved shared libraries")
    return {"scope": "Packaging-host runtime observations, not a claim about another build host",
            "pkg_config": versions, "ldd": linked,
            "distribution": "No shared libraries are bundled; the target distribution supplies their licenses and sources"}


def native_artifact(repository: Path, revision: str, binary: Path,
                    stage: Path, output: Path) -> Path:
    metadata = identity(repository, revision)
    validate_prepared(repository, stage, revision)
    binary = binary.resolve(strict=True)
    with binary.open("rb") as stream:
        if stream.read(4) != b"\x7fELF":
            raise ValueError("Provide the actual compiled Native ELF")
    version = verify_binary(binary, metadata, "Native")
    compiler, target = binary_identity(version)
    runtime = runtime_entries(compiler, target)
    output = output_directory(output)
    source = source_artifact(repository, revision, output, stage)
    info = {**build_info(metadata, "Native Linux tar archive"), "binary_sha256": sha256(binary),
            "binary_version": version, "rust_runtime": json.loads(runtime["BUILD-INFO.json"]),
            "packaging_host_architecture": platform.machine(),
            "native_libraries": native_libraries(binary),
            "corresponding_source": {"file": source.name, "sha256": sha256(source)}}
    entries = {"karin": binary, "BUILD-INFO.json": json_bytes(info)}
    for source_name, target in {
        f"packaging/{APP_ID}.desktop": f"share/applications/{APP_ID}.desktop",
        f"packaging/{APP_ID}.metainfo.xml": f"share/metainfo/{APP_ID}.metainfo.xml",
        "LICENSE": f"share/licenses/{APP_ID}/LICENSE",
        "docs/LICENSING.md": f"share/licenses/{APP_ID}/LICENSING.md",
        "docs/BUILDING.md": "BUILDING.md",
    }.items():
        entries[target] = stage / source_name
    for size in APP_ICON_SIZES:
        icon = f"hicolor/{size}x{size}/apps/{APP_ID}.png"
        entries[f"share/icons/{icon}"] = stage / f"packaging/icons/{icon}"
    folder = stage / "third-party-notices"
    for path in tree_files(folder):
        entries[f"share/licenses/{APP_ID}/third-party/{path.relative_to(folder)}"] = path
    # UI assets are embedded in the ELF; their original notices are separate
    # files. Admit only committed upstream notices, never private stage extras.
    prefix = "packaging/third-party-licenses/"
    for name in sorted(committed_files(repository, revision)):
        if name.startswith(prefix):
            entries[f"share/licenses/{APP_ID}/{name.removeprefix(prefix)}"] = stage / name
    entries.update({f"share/licenses/{APP_ID}/rust-runtime/{name}": data for name, data in runtime.items()})
    entries["run-karin"] = b'#!/bin/sh\nset -eu\nroot=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)\nexport XDG_DATA_DIRS="$root/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"\nexec "$root/karin" "$@"\n'
    entries["NATIVE-LIBRARIES.json"] = json_bytes(info["native_libraries"])
    target = output / f"{artifact_name(metadata)}-linux-{platform.machine()}.tar.gz"
    archive(target, entries, metadata["source_date_epoch"], {"karin", "run-karin"})
    identity(repository, revision)
    write_json(output / f"{target.name.removesuffix('.tar.gz')}.BUILD-INFO.json", info)
    return target


def validate_permissions(raw: str) -> None:
    config = configparser.ConfigParser(interpolation=None)
    try:
        config.read_string(raw)
    except configparser.Error as error:
        raise ValueError("Invalid installed Flatpak permission metadata") from error
    if not config.has_section("Context"):
        raise ValueError("Installed Flatpak has no permission context")
    allowed = {"shared": {"network", "ipc"}, "sockets": {"wayland", "x11", "fallback-x11", "pulseaudio"},
               "devices": {"dri"}, "filesystems": set(DISCORD_FILESYSTEMS)}
    for key, value in config.items("Context"):
        values = set(value.split(";")) - {""}
        if values and (key not in allowed or not values <= allowed[key]):
            raise ValueError(f"Unexpected installed Flatpak permission: {key}")
    filesystems = set(config.get("Context", "filesystems", fallback="").split(";")) - {""}
    if filesystems != allowed["filesystems"]:
        raise ValueError("Installed Flatpak Discord permissions differ from the scoped runtime paths")
    bus = dict(config.items("Session Bus Policy")) if config.has_section("Session Bus Policy") else {}
    if bus != {f"org.mpris.MediaPlayer2.{APP_ID}".lower(): "own"}:
        raise ValueError("Installed Flatpak bus permissions differ from scoped MPRIS ownership")
    if config.has_section("System Bus Policy") and dict(config.items("System Bus Policy")):
        raise ValueError("Unexpected system bus permission")
    if config.has_option("Environment", "WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS"):
        raise ValueError("WebKit sandbox must not be disabled")


def runtime_commits(manifest: dict) -> dict:
    """Resolve SDK extensions through its selected extension-point branch."""
    sdk = capture(["flatpak", "info", "--user", "--show-ref", f"{manifest['sdk']}//{manifest['runtime-version']}"])
    parts = sdk.split("/")
    if len(parts) != 4 or parts[0] != "runtime" or parts[1] != manifest["sdk"]:
        raise ValueError("Unexpected selected SDK reference")
    _, _, architecture, sdk_branch = parts
    runtime = capture(["flatpak", "info", "--user", "--show-ref", f"{manifest['runtime']}//{manifest['runtime-version']}"])
    config = configparser.ConfigParser(interpolation=None)
    config.read_string(capture(["flatpak", "info", "--user", "--show-metadata", sdk]))
    refs = [runtime, sdk]
    for extension in manifest.get("sdk-extensions", []):
        points = []
        for section in config.sections():
            if not section.startswith("Extension "):
                continue
            name = section.removeprefix("Extension ")
            if extension == name or (extension.startswith(name + ".") and config.getboolean(section, "subdirectories", fallback=False)):
                points.append((len(name), section))
        if not points:
            raise ValueError(f"SDK has no extension point for {extension}")
        section = max(points)[1]
        branch = config.get(section, "version", fallback="")
        if not branch:
            alternatives = [v for v in config.get(section, "versions", fallback="").split(";") if v]
            if len(alternatives) > 1:
                raise ValueError("SDK extension branch is ambiguous; review the selected extension point")
            branch = alternatives[0] if alternatives else sdk_branch
        if not re.fullmatch(r"[A-Za-z0-9._-]+", branch):
            raise ValueError("Unsafe SDK extension branch")
        refs.append(f"runtime/{extension}/{architecture}/{branch}")
    return {ref: capture(["flatpak", "info", "--user", "--show-commit", ref]) for ref in refs}


def installed_licenses(stage: Path, location: Path, version: str) -> dict:
    root = location / "files/share/licenses" / APP_ID
    if not (root / "third-party").is_dir():
        raise ValueError("Installed Flatpak is missing its canonical application license directory")
    for name, source in (("LICENSE", "LICENSE"), ("LICENSING.md", "docs/LICENSING.md")):
        if sha256(root / name) != sha256(stage / source):
            raise ValueError("Installed project licensing differs from the release source")
    crates = locked_crates(tomllib.loads((stage / "Cargo.lock").read_text()))
    expected = {f"{p['name']}-{p['version']}" for p in crates}
    notices = root / "third-party"
    if {path.name for path in notices.iterdir() if path.is_dir()} != expected or not (notices / "DEPENDENCY_LICENSES.md").is_file():
        raise ValueError("Installed crate notice inventory is incomplete")
    actual = {str(path.relative_to(notices)): sha256(path) for path in tree_files(notices)}
    source_notices = stage / "third-party-notices"
    required = {str(path.relative_to(source_notices)): sha256(path) for path in tree_files(source_notices)}
    if actual != required:
        raise ValueError("Installed crate notice bytes differ from authenticated prepared source")
    upstream = stage / "packaging/third-party-licenses"
    upstream_names = []
    for folder in sorted(upstream.iterdir()):
        required = {str(path.relative_to(folder)): sha256(path) for path in tree_files(folder)}
        actual = {str(path.relative_to(root / folder.name)): sha256(path)
                  for path in tree_files(root / folder.name)}
        if actual != required:
            raise ValueError("Installed upstream notice bytes differ from release source")
        upstream_names.append(folder.name)
    compiler, target = binary_identity(version)
    runtime = validate_runtime(root / "rust-runtime", compiler, target)
    return {"path_in_application": str(root.relative_to(location)), "crate_notice_directories": len(expected),
            "project_license_and_cassette_provenance_verified": True,
            "upstream_notice_directories": upstream_names, "rust_runtime": runtime}


def flatpak_artifact(repository: Path, revision: str, manifest_path: Path, output: Path,
                     stage: Path | None = None) -> Path:
    metadata = identity(repository, revision)
    manifest = json.loads(manifest_path.read_text())
    source = provenance(manifest, metadata, repository)
    output = output_directory(output)
    if stage is None and (repository / "build/flatpak-source").is_dir():
        stage = repository / "build/flatpak-source"
    corresponding_source = source_artifact(repository, revision, output, stage)
    stage = stage or repository / "build/flatpak-source"
    branch = manifest.get("branch", "master")
    app = f"{APP_ID}//{branch}"
    repository_path = output / "flatpak-repository"
    subprocess.run(["flatpak-builder", "--user", "--install", "--force-clean", "--disable-rofiles-fuse",
                    f"--repo={repository_path}", f"--state-dir={output / 'flatpak-state'}",
                    str(output / "flatpak-build"), str(manifest_path.resolve())], check=True)
    version = validate_version_output(capture(["flatpak", "run", "--user", app, "--version"]), metadata, "Flatpak")
    location = Path(capture(["flatpak", "info", "--user", "--show-location", app]))
    permissions = capture(["flatpak", "info", "--user", "--show-permissions", app])
    validate_permissions(permissions)
    validate_prepared(repository, stage, revision)
    info = {**build_info(metadata, "locally installed SDK Flatpak and bundle"), "source": source,
            "manifest_sha256": sha256(manifest_path), "binary_version": version,
            "binary_sha256": sha256(location / "files/bin/karin"), "permissions": permissions,
            "installed_commit": capture(["flatpak", "info", "--user", "--show-commit", app]),
            "runtime_commits": runtime_commits(manifest), "licenses": installed_licenses(stage, location, version),
            "corresponding_source": {"file": corresponding_source.name, "sha256": sha256(corresponding_source)},
            "flatpak": capture(["flatpak", "--version"]),
            "flatpak_builder": capture(["flatpak-builder", "--version"])}
    target = output / f"{artifact_name(metadata)}.flatpak"
    if target.exists():
        raise ValueError("Use a fresh Flatpak bundle output path")
    subprocess.run(["flatpak", "build-bundle", str(repository_path), str(target), APP_ID, branch], check=True)
    identity(repository, revision)
    write_json(output / f"{artifact_name(metadata)}-flatpak.BUILD-INFO.json", info)
    return target


def checksums(output: Path, files: list[Path]) -> None:
    paths = [path.resolve(strict=True) for path in files]
    if len(set(paths)) != len(paths) or output.resolve() in paths:
        raise ValueError("Checksums need a unique explicit artifact list excluding the checksum file")
    records = []
    for path in sorted(paths):
        if not path.is_file() or any(c in str(path) for c in "\r\n\\"):
            raise ValueError("Unsupported checksum input path")
        name = str(path.relative_to(output.resolve().parent)) if path.is_relative_to(output.resolve().parent) else str(path)
        records.append(f"{sha256(path)}  {name}\n")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("".join(records))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("source", "native", "flatpak"):
        command = commands.add_parser(name)
        command.add_argument("--revision", required=True)
        command.add_argument("--output", type=Path, required=True)
        if name == "source":
            command.add_argument("--prepared", type=Path)
        elif name == "native":
            command.add_argument("--binary", type=Path, required=True)
            command.add_argument("--source-dir", type=Path, required=True)
        else:
            command.add_argument("--manifest", type=Path, required=True)
            command.add_argument("--source-dir", type=Path,
                                 help="Reuse an authenticated prepared source; otherwise use/build build/flatpak-source")
    command = commands.add_parser("checksums")
    command.add_argument("--output", type=Path, required=True)
    command.add_argument("files", type=Path, nargs="+")
    args = parser.parse_args()
    try:
        if args.command == "source":
            print(source_artifact(REPOSITORY, args.revision, args.output, args.prepared))
        elif args.command == "native":
            print(native_artifact(REPOSITORY, args.revision, args.binary, args.source_dir, args.output))
        elif args.command == "flatpak":
            print(flatpak_artifact(REPOSITORY, args.revision, args.manifest, args.output, args.source_dir))
        else:
            checksums(args.output, args.files)
    except (ValueError, OSError, tarfile.TarError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Artifact preparation failed: {error}\n")


if __name__ == "__main__":
    main()
