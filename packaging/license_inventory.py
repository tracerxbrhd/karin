#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Inventory locked crate declarations and optionally preserve their notices.

This records a reviewed lockfile; it does not replace upstream license texts or
certify an entire native/runtime distribution. Unknown declarations require review.
Run with the same Cargo toolchain and trusted registry configuration as the build.
"""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


REVIEWED_EXPRESSIONS = {
    "(MIT OR Apache-2.0) AND Unicode-3.0",
    "Apache-2.0",
    "Apache-2.0 AND ISC",
    "Apache-2.0 OR BSL-1.0",
    "Apache-2.0 OR ISC OR MIT",
    "Apache-2.0 OR MIT",
    "Apache-2.0 WITH LLVM-exception",
    "Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT",
    "BSD-3-Clause",
    "ISC",
    "MIT",
    "MIT OR Apache-2.0",
    "MIT OR Apache-2.0 OR LGPL-2.1-or-later",
    "MIT OR Apache-2.0 OR Zlib",
    "MIT/Apache-2.0",  # Legacy Cargo spelling; both upstream license files exist.
    "Unicode-3.0",
    "Unlicense OR MIT",
    "Zlib OR Apache-2.0 OR MIT",
}

# These four published 0.5.0 archives omit their repository's MIT license.
# Review: packaging/third-party-licenses/webkit6-rs-0.5.0/PROVENANCE.md.
# No name/version, source commit or notice-content changes are accepted implicitly.
WEBKIT6_REPOSITORY = "https://gitlab.gnome.org/World/Rust/webkit6-rs"
WEBKIT6_COMMIT = "545df16de5ca5bba4b1b8b456f75a508c98d5ba2"
WEBKIT6_LICENSE_SHA256 = "44dad7b2e199b0d355adf4437df5c9bb18633d47804242f24dab5a633faba5f7"
REVIEWED_NOTICE_FALLBACKS = {
    ("javascriptcore6", "0.5.0"): "javascriptcore6",
    ("javascriptcore6-sys", "0.5.0"): "javascriptcore6/sys",
    ("webkit6", "0.5.0"): "webkit6",
    ("webkit6-sys", "0.5.0"): "webkit6/sys",
}


def metadata(repository: Path, target: str | None = None) -> dict:
    command = ["cargo", "metadata", "--locked", "--all-features", "--format-version", "1"]
    if target:
        command += ["--filter-platform", target]
    return json.loads(subprocess.check_output(command, cwd=repository, text=True))


def notice_files(package: dict, repository: Path) -> list[tuple[Path, Path]]:
    root = Path(package["manifest_path"]).parent
    prefixes = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT", "AUTHORS")
    files = {
        path
        for path in root.rglob("*")
        if path.is_file() and path.name.upper().startswith(prefixes)
    }
    if package.get("license_file"):
        files.add(root / package["license_file"])
    if files:
        return [(path, path.relative_to(root)) for path in sorted(files)]

    vcs_path = REVIEWED_NOTICE_FALLBACKS.get((package["name"], package["version"]))
    if vcs_path is None:
        return []
    label = f"{package['name']} {package['version']}"
    try:
        vcs = json.loads((root / ".cargo_vcs_info.json").read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise SystemExit(f"Missing/invalid reviewed notice provenance: {label}") from error
    if (
        package.get("repository") != WEBKIT6_REPOSITORY
        or package.get("license") != "MIT"
        or vcs.get("git", {}).get("sha1") != WEBKIT6_COMMIT
        or vcs.get("path_in_vcs") != vcs_path
    ):
        raise SystemExit(f"Unreviewed source for notice fallback: {label}")

    fallback = repository / "packaging/third-party-licenses/webkit6-rs-0.5.0"
    license_file = fallback / "LICENSE"
    provenance = fallback / "PROVENANCE.md"
    if not license_file.is_file() or not provenance.is_file():
        raise SystemExit(f"Missing reviewed upstream notice fallback: {label}")
    if hashlib.sha256(license_file.read_bytes()).hexdigest() != WEBKIT6_LICENSE_SHA256:
        raise SystemExit(f"Changed reviewed upstream notice fallback: {label}")
    return [(license_file, Path("LICENSE")), (provenance, Path("UPSTREAM_LICENSE_SOURCE.md"))]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="Write a deterministic Markdown inventory")
    parser.add_argument("--notices-dir", type=Path, help="Create a NEW third-party notices directory")
    arguments = parser.parse_args()
    repository = Path(__file__).resolve().parents[1]
    all_targets = metadata(repository)
    linux = metadata(repository, "x86_64-unknown-linux-gnu")
    workspace = set(all_targets["workspace_members"])
    packages = sorted(
        (p for p in all_targets["packages"] if p["id"] not in workspace),
        key=lambda p: (p["name"], p["version"]),
    )
    linux_ids = {node["id"] for node in linux["resolve"]["nodes"]}
    unknown = [
        f"{p['name']} {p['version']}: {p.get('license')!r}"
        for p in packages
        if p.get("license") not in REVIEWED_EXPRESSIONS
    ]
    if unknown:
        raise SystemExit("Unreviewed or missing license declarations:\n" + "\n".join(unknown))

    # Preserve supplied notices, or an explicitly reviewed/hash-pinned upstream
    # text for an archive omission. Never synthesize license text from its SPDX ID.
    notices = {p["id"]: notice_files(p, repository) for p in packages}
    missing = [f"{p['name']} {p['version']}" for p in packages if not notices[p["id"]]]
    if missing:
        raise SystemExit("No supplied license/notice file found; review required:\n" + "\n".join(missing))

    lines = [
        "<!-- SPDX-License-Identifier: GPL-3.0-or-later -->",
        "# Locked Rust dependency license inventory",
        "",
        "Generated by `python3 packaging/license_inventory.py --output docs/DEPENDENCY_LICENSES.md`.",
        "See [LICENSING.md](LICENSING.md) for reviewed choices, obligations and scope limits.",
        "Declarations remain upstream licenses, not GPL relicensing by Karin.",
        "",
        "Scope: `cargo metadata --locked --all-features` includes optional authentication bindings.",
        "The four WebKit/JavaScriptCore 0.5.0 archive notice omissions use an exact-commit,",
        "SHA-256-verified upstream MIT text; see the [reviewed provenance](../packaging/third-party-licenses/webkit6-rs-0.5.0/PROVENANCE.md).",
        "",
        f"External packages: **{len(packages)}** across the lockfile; "
        f"**{sum(p['id'] in linux_ids for p in packages)}** in Cargo's Linux target graph "
        "(including build dependencies). A resolved graph is not a binary SBOM.",
        "",
        "| Crate | Version | Upstream declaration | Linux graph |",
        "| --- | --- | --- | --- |",
    ]
    for package in packages:
        name, version = package["name"], package["version"]
        lines.append(
            f"| [{name}](https://crates.io/crates/{name}/{version}) | {version} | "
            f"`{package['license']}` | {'yes' if package['id'] in linux_ids else 'no'} |"
        )
    document = "\n".join(lines) + "\n"
    if arguments.output:
        arguments.output.write_text(document, encoding="utf-8")
    else:
        print(document, end="")

    if arguments.notices_dir:
        destination = arguments.notices_dir
        # Refuse destructive overwrites and preserve all original relative paths.
        destination.mkdir(parents=True, exist_ok=False)
        (destination / "DEPENDENCY_LICENSES.md").write_text(document, encoding="utf-8")
        for package in packages:
            output = destination / f"{package['name']}-{package['version']}"
            for source, relative in notices[package["id"]]:
                target = output / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, target)


if __name__ == "__main__":
    main()
