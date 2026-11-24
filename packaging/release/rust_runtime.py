#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Collect notices from the compiler that supplied a binary's Rust runtime."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tomllib

LICENSES = (
    "Apache-2.0.txt", "BSD-2-Clause.txt", "CC-BY-SA-4.0.txt",
    "GCC-exception-3.1.txt", "GPL-2.0-only.txt", "GPL-3.0-or-later.txt",
    "ISC.txt", "LLVM-exception.txt", "MIT.txt", "NCSA.txt",
    "OFL-1.1.txt", "Unicode-3.0.txt",
)
NOTICE_LIMITS = {"COPYRIGHT-library.html": 4 * 1024 * 1024,
                 "compiler-builtins/LICENSE.txt": 256 * 1024,
                 **{f"licenses/{name}": 512 * 1024 for name in LICENSES}}


def _command(*arguments: str) -> str:
    return subprocess.check_output(["rustc", *arguments], text=True).strip()


def _read(root: Path, relative: str, maximum: int) -> bytes:
    path = root
    for part in Path(relative).parts:
        if part in ("", ".", ".."):
            raise ValueError("Unsafe Rust notice path")
        path /= part
        if path.is_symlink():
            raise ValueError(f"Rust notice must not follow a symlink: {relative}")
    if not path.is_file() or not 0 < path.stat().st_size <= maximum:
        raise ValueError(f"Missing or oversized required Rust notice: {relative}")
    return path.read_bytes()


def _compiler_identity(compiler: str, verbose: str) -> str:
    lines = verbose.splitlines()
    if not lines or lines[0] != compiler:
        raise ValueError("Rust compiler verbose identity differs")
    hashes = [line.removeprefix("commit-hash: ") for line in lines if line.startswith("commit-hash: ")]
    match = re.fullmatch(r"rustc \S+ \(([0-9a-f]{7,40}) \d{4}-\d{2}-\d{2}\)", compiler)
    if len(hashes) != 1 or not re.fullmatch(r"[0-9a-f]{40}", hashes[0]) or not match or not hashes[0].startswith(match[1]):
        raise ValueError("Rust compiler needs a matching full source commit")
    return hashes[0]


def binary_identity(raw: str) -> tuple[str, str]:
    values = []
    for prefix in ("Compiler: ", "Target: "):
        found = [line.removeprefix(prefix) for line in raw.splitlines() if line.startswith(prefix)]
        if len(found) != 1 or not found[0]:
            raise ValueError("Binary must report one compiler and one target")
        values.append(found[0])
    return values[0], values[1]


def runtime_entries(expected_compiler: str, target: str) -> dict[str, bytes]:
    if not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.-]*", target):
        raise ValueError("Use the binary's standard Rust target triple")
    compiler = _command("--version")
    if compiler != expected_compiler:
        raise ValueError("Current rustc does not match the binary's embedded compiler")
    verbose = _command("-vV")
    commit = _compiler_identity(compiler, verbose)
    sysroot = Path(_command("--print", "sysroot")).resolve(strict=True)
    target_libdir = Path(_command("--print", "target-libdir", "--target", target)).resolve(strict=True)
    expected_libdir = sysroot / "lib/rustlib" / target / "lib"
    if target_libdir != expected_libdir or not target_libdir.is_dir():
        raise ValueError("Rust target libraries do not belong to the selected sysroot")
    if not list(target_libdir.glob("libstd-*.rlib")) or not list(target_libdir.glob("libcompiler_builtins-*.rlib")):
        raise ValueError("Install the compiler-matched standard library for the binary's target")
    docs = sysroot / "share/doc/rust"
    license_directory = docs / "licenses"
    if license_directory.is_symlink() or {p.name for p in license_directory.iterdir() if p.suffix == ".txt"} != set(LICENSES):
        raise ValueError("Review changed compiler-supplied license inventory")
    entries = {"COPYRIGHT-library.html": _read(sysroot, "share/doc/rust/COPYRIGHT-library.html", NOTICE_LIMITS["COPYRIGHT-library.html"])}
    for name in LICENSES:
        entries[f"licenses/{name}"] = _read(sysroot, f"share/doc/rust/licenses/{name}", NOTICE_LIMITS[f"licenses/{name}"])
    source = "lib/rustlib/src/rust/library/compiler-builtins"
    cargo_bytes = _read(sysroot, f"{source}/compiler-builtins/Cargo.toml", 128 * 1024)
    package = tomllib.loads(cargo_bytes.decode())["package"]
    if package["name"] != "compiler_builtins" or not re.fullmatch(r"[A-Za-z0-9.+_-]+", package["version"]):
        raise ValueError("Unexpected compiler-builtins source metadata")
    entries["compiler-builtins/LICENSE.txt"] = _read(sysroot, f"{source}/LICENSE.txt", NOTICE_LIMITS["compiler-builtins/LICENSE.txt"])
    info = {"schema": 1, "compiler": compiler, "compiler_verbose": verbose,
            "target": target, "compiler_commit": commit,
            "target_library_subdir": f"lib/rustlib/{target}/lib",
            "rust_source_url": f"https://github.com/rust-lang/rust/tree/{commit}",
            "compiler_builtins": {"version": package["version"], "license": package["license"],
                                  "source_path": "library/compiler-builtins",
                                  "cargo_toml_sha256": hashlib.sha256(cargo_bytes).hexdigest()},
            "files": {name: {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
                      for name, data in sorted(entries.items())}}
    entries["BUILD-INFO.json"] = (json.dumps(info, indent=2, sort_keys=True) + "\n").encode()
    return entries


def validate_runtime(directory: Path, expected_compiler: str, target: str) -> dict:
    """Check payload integrity/declarations; build collector establishes origin."""
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError("Expected a regular Rust runtime notice directory")
    raw = _read(directory, "BUILD-INFO.json", 128 * 1024)
    info = json.loads(raw)
    if info.get("schema") != 1 or info.get("compiler") != expected_compiler or info.get("target") != target:
        raise ValueError("Rust runtime compiler/target differs from the binary")
    commit = _compiler_identity(expected_compiler, info["compiler_verbose"])
    if info.get("compiler_commit") != commit or info.get("rust_source_url") != f"https://github.com/rust-lang/rust/tree/{commit}":
        raise ValueError("Rust runtime source provenance differs")
    if info.get("target_library_subdir") != f"lib/rustlib/{target}/lib" or set(info["files"]) != set(NOTICE_LIMITS):
        raise ValueError("Rust runtime notice inventory differs")
    builtins = info["compiler_builtins"]
    if builtins.get("source_path") != "library/compiler-builtins" or not re.fullmatch(r"[A-Za-z0-9.+_-]+", builtins.get("version", "")) or not re.fullmatch(r"[0-9a-f]{64}", builtins.get("cargo_toml_sha256", "")):
        raise ValueError("Rust compiler-builtins provenance differs")
    found = set()
    for path in directory.rglob("*"):
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ValueError("Rust runtime notices contain a link/special entry")
        if path.is_file():
            found.add(str(path.relative_to(directory)))
    if found != set(NOTICE_LIMITS) | {"BUILD-INFO.json"}:
        raise ValueError("Rust runtime notices contain missing or extra files")
    for name, maximum in NOTICE_LIMITS.items():
        data = _read(directory, name, maximum)
        if info["files"][name] != {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}:
            raise ValueError(f"Changed Rust runtime notice: {name}")
    return info


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--binary", type=Path)
    group.add_argument("--expected-compiler")
    parser.add_argument("--target")
    args = parser.parse_args()
    if args.binary:
        if args.target is not None:
            parser.error("--binary supplies its actual target; do not override it")
        compiler, target = binary_identity(subprocess.check_output([str(args.binary.resolve()), "--version"], text=True))
    else:
        if args.target is None:
            parser.error("--target is required with --expected-compiler")
        compiler, target = args.expected_compiler, args.target
    if args.output.exists() or args.output.is_symlink():
        parser.error("Use a fresh Rust runtime notice directory")
    entries = runtime_entries(compiler, target)
    args.output.mkdir(parents=True)
    for name, data in entries.items():
        path = args.output / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    validate_runtime(args.output, compiler, target)
    print(args.output)


if __name__ == "__main__":
    main()
