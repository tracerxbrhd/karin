#!/usr/bin/env python3
"""Prepare a local source snapshot and locked Cargo dependencies for Flatpak.

Only build/flatpak-source is replaced. No repository Cargo configuration, source,
dependency declaration or lockfile is modified.
"""

from pathlib import Path
import shutil
import subprocess


def main() -> None:
    repository = Path(__file__).resolve().parents[2]
    if not (repository / "Cargo.lock").is_file():
        raise SystemExit("Cargo.lock is required; commit the reviewed dependency lockfile first.")
    build = repository / "build"
    if build.is_symlink():
        raise SystemExit("Refusing to write through a symlinked build directory.")
    build.mkdir(exist_ok=True)
    stage = build / "flatpak-source"
    if stage.is_symlink():
        raise SystemExit("Refusing to replace a symlinked source directory.")
    if stage.exists():
        shutil.rmtree(stage)
    shutil.copytree(
        repository,
        stage,
        ignore=shutil.ignore_patterns(
            ".git", "target", "build", "builddir", ".flatpak-builder", ".env", ".env.*", "*.log"
        ),
    )
    cargo_config = stage / ".cargo" / "config.toml"
    cargo_config.parent.mkdir(exist_ok=True)
    with cargo_config.open("w", encoding="utf-8") as configuration:
        subprocess.run(
            ["cargo", "vendor", "--locked", "--versioned-dirs", "vendor"],
            cwd=stage,
            stdout=configuration,
            check=True,
        )
    print(f"Prepared {stage}; review the snapshot before building or distributing it.")


if __name__ == "__main__":
    main()
