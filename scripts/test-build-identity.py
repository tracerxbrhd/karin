#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Exercise Karin's actual build.rs in small, dependency-free Cargo projects.

Checks native Git revision changes and source-only package metadata while
requiring unchanged builds to stay fresh. Runs without account or network.
"""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--toolchain")
parser.add_argument(
    "--build-script",
    type=Path,
    default=Path(__file__).resolve().parents[1] / "build.rs",
)
args = parser.parse_args()
cargo = ["cargo"] + ([f"+{args.toolchain}"] if args.toolchain else [])
rustc = ["rustc"] + ([f"+{args.toolchain}"] if args.toolchain else [])


def command(arguments, directory, environment=None):
    result = subprocess.run(
        arguments,
        cwd=directory,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0, (
        f"Command failed ({arguments[0]}):\n{result.stdout}\n{result.stderr}"
    )
    return result.stdout


def build(project):
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(project / "target")
    output = command(
        cargo + ["build", "--offline", "--locked", "--message-format=json"],
        project,
        environment,
    )
    messages = [json.loads(line) for line in output.splitlines()]
    binaries = [
        message
        for message in messages
        if message.get("reason") == "compiler-artifact"
        and message.get("target", {}).get("kind") == ["bin"]
    ]
    assert len(binaries) == 1, "One compiled probe executable is required"
    artifact = binaries[0]
    identity = dict(
        line.split("=", 1)
        for line in command([artifact["executable"]], project).splitlines()
    )
    return artifact["fresh"], identity


def require_fresh(project, expected):
    for _ in range(2):
        fresh, identity = build(project)
        assert fresh, "Unchanged source rebuilt; a missing watched path invalidates Cargo"
        assert identity == expected, "A fresh artifact changed its embedded identity"


with tempfile.TemporaryDirectory(prefix="karin-build-identity-") as temporary:
    root = Path(temporary)
    project = root / "native"
    project.mkdir()
    for name in ["src", "resources", "packaging"]:
        (project / name).mkdir()
        (project / name / ".keep").write_text("Original build identity fixture\n")
    shutil.copyfile(args.build_script, project / "build.rs")
    (project / ".gitignore").write_text("/target/\n")
    (project / "Cargo.toml").write_text(
        '[package]\nname = "karin-build-identity-probe"\n'
        'version = "0.0.0"\nedition = "2024"\n'
    )
    (project / "src" / "main.rs").write_text(
        'fn main() { println!("revision={}\\nstate={}\\nprofile={}", '
        'env!("KARIN_SOURCE_REVISION"), env!("KARIN_SOURCE_STATUS"), '
        'env!("KARIN_BUILD_PROFILE")); }\n'
    )
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(project / "target")
    command(cargo + ["generate-lockfile", "--offline"], project, environment)
    command(["git", "init", "--initial-branch=acceptance"], project)
    command(["git", "config", "user.name", "Karin build fixture"], project)
    command(["git", "config", "user.email", "fixture@example.invalid"], project)
    command(["git", "add", "."], project)
    command(["git", "commit", "-m", "Original build identity fixture"], project)
    revision = command(["git", "rev-parse", "HEAD"], project).strip()
    fresh, identity = build(project)
    assert not fresh
    assert identity == {"revision": revision, "state": "clean", "profile": "debug"}
    require_fresh(project, identity)

    # An empty commit proves the Git-ref watch, independently of source mtimes.
    command(["git", "commit", "--allow-empty", "-m", "Change only Git revision"], project)
    revision = command(["git", "rev-parse", "HEAD"], project).strip()
    fresh, identity = build(project)
    assert not fresh and identity["revision"] == revision
    assert identity["state"] == "clean"
    require_fresh(project, identity)

    # Packing removes the loose branch file. Missing loose refs must not
    # invalidate every build, and the next commit must still update identity.
    command(["git", "pack-refs", "--all", "--prune"], project)
    fresh, packed_identity = build(project)
    assert not fresh and packed_identity == identity
    require_fresh(project, identity)
    command(["git", "commit", "--allow-empty", "-m", "Advance packed branch"], project)
    revision = command(["git", "rev-parse", "HEAD"], project).strip()
    fresh, identity = build(project)
    assert not fresh and identity["revision"] == revision
    assert identity["state"] == "clean"
    require_fresh(project, identity)

    # Changes to watched runtime source still report a modified working tree.
    with (project / "src" / "main.rs").open("a") as source:
        source.write("// Original fixture source modification\n")
    fresh, identity = build(project)
    assert not fresh and identity["revision"] == revision
    assert identity["state"] == "modified"
    require_fresh(project, identity)

    snapshot = root / "source-only"
    shutil.copytree(
        project, snapshot, ignore=shutil.ignore_patterns(".git", "target")
    )
    (snapshot / "BUILD-REVISION").write_text("a" * 40 + "\n")
    (snapshot / "BUILD-SOURCE-STATUS").write_text("clean\n")
    fresh, identity = build(snapshot)
    assert not fresh
    assert identity == {"revision": "a" * 40, "state": "clean", "profile": "debug"}
    require_fresh(snapshot, identity)

    (snapshot / "BUILD-REVISION").write_text("b" * 40 + "\n")
    fresh, identity = build(snapshot)
    assert not fresh and identity["revision"] == "b" * 40
    assert identity["state"] == "clean"
    require_fresh(snapshot, identity)

    (snapshot / "BUILD-SOURCE-STATUS").write_text("modified\n")
    fresh, identity = build(snapshot)
    assert not fresh and identity["state"] == "modified"
    assert identity["revision"] == "b" * 40
    require_fresh(snapshot, identity)

    compiler = command(rustc + ["--version"], project).strip()
    print(
        json.dumps(
            {
                "result": "PASS",
                "compiler": compiler,
                "native_unchanged_is_fresh": True,
                "git_revision_updates": True,
                "packed_git_is_fresh_and_updates": True,
                "source_modification_updates": True,
                "source_only_metadata_updates": True,
                "source_only_unchanged_is_fresh": True,
            }
        )
    )
