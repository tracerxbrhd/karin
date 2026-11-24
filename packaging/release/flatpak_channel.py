#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Prepare, verify and package a signed stable Flatpak channel snapshot."""

import argparse
import base64
import configparser
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from artifacts import artifact_name, capture, validate_permissions
from common import APP_ID, REPOSITORY, identity, sha256, validate_version_output, write_json

CHANNEL_URL = "https://tracerxbrhd.github.io/karin/flatpak/"
RUNTIME_REPO = "https://flathub.org/repo/flathub.flatpakrepo"
APP_REF = f"app/{APP_ID}/x86_64/master"
PIN = "packaging/release/flatpak-key-fingerprint"
UPDATE_RECORD = "KARIN-FLATPAK-UPDATE.json"
SITE_LIMIT = 250 * 1024**2
STABLE_VERSION = r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"


def ostree(repo: Path, *args: str) -> list[str]:
    return ["ostree", f"--repo={repo}", *args]


def trusted_key(home: Path, fingerprint: str) -> bytes:
    if not re.fullmatch(r"[A-F0-9]{40}", fingerprint):
        raise ValueError("A reviewed stable signing fingerprint must be pinned in source")
    command = ["gpg", "--batch", "--no-options", "--homedir", str(home)]
    listing = capture([*command, "--with-colons", "--fingerprint", "--list-keys", fingerprint])
    public_keys = [line.split(":") for line in listing.splitlines() if line.startswith("pub:")]
    fingerprints = [line.split(":")[9] for line in listing.splitlines() if line.startswith("fpr:")]
    if len(public_keys) != 1 or not fingerprints or fingerprints[0] != fingerprint:
        raise ValueError("The signing keyring must provide the pinned primary key")
    key = subprocess.check_output([*command, "--export-options", "export-minimal", "--export", fingerprint])
    if not key or len(key) > 32 * 1024:
        raise ValueError("Invalid public key export")
    return key


def validate_export(repo: Path, receipt: dict, build: dict, work: Path) -> str:
    for key, expected in build.items():
        if receipt.get(key) != expected:
            raise ValueError("Flatpak artifact receipt does not match the reviewed source identity")
    if receipt.get("app_id") != APP_ID:
        raise ValueError("Unexpected artifact application ID")
    validate_version_output(receipt["binary_version"], build, "Flatpak")
    commit = capture(ostree(repo, "rev-parse", APP_REF))
    if not re.fullmatch(r"[a-f0-9]{64}", commit) or receipt.get("installed_commit") != commit:
        raise ValueError("Exported ref differs from the installed and verified artifact commit")
    checkout = work / "validated-export"
    subprocess.run(ostree(repo, "checkout", "--user-mode", commit, str(checkout)), check=True)
    binary = checkout / "files/bin/karin"
    metadata_file = checkout / "metadata"
    if any(path.is_symlink() or not path.is_file() for path in (binary, metadata_file)):
        raise ValueError("Expected regular exported executable and sandbox metadata")
    with binary.open("rb") as executable:
        if executable.read(4) != b"\x7fELF":
            raise ValueError("Expected an exported ELF executable")
    if sha256(binary) != receipt.get("binary_sha256"):
        raise ValueError("The exported binary differs from the verified installation")
    metadata = metadata_file.read_text(encoding="utf-8")
    validate_permissions(metadata)
    parser = configparser.ConfigParser(interpolation=None)
    parser.read_string(metadata)
    if (parser.get("Application", "name") != APP_ID
            or parser.get("Application", "runtime") != "org.gnome.Platform/x86_64/50"):
        raise ValueError("The stable update ref and runtime must match the installed app")
    return commit


def verify_signed_pull(repo: Path, work: Path, public_key: Path, commit: str) -> None:
    """Import into an independent repository using only the distributed public key."""
    verified = work / "verified"
    subprocess.run(ostree(verified, "init", "--mode=archive"), check=True)
    subprocess.run(ostree(
        verified, "remote", "add", f"--gpg-import={public_key}",
        "--set=gpg-verify=true", "--set=gpg-verify-summary=true", "karin",
        repo.as_uri(), APP_REF,
    ), check=True)
    subprocess.run(ostree(verified, "pull", "--untrusted", "karin", APP_REF), check=True)
    if capture(ostree(verified, "rev-parse", f"karin:{APP_REF}")) != commit:
        raise ValueError("Signed repository does not deliver the verified deployment")


def verify_signed_bundle(bundle: Path, work: Path, public_key: Path, commit: str) -> None:
    imported = work / "bundle-import"
    verified = work / "bundle-verified"
    for repo in (imported, verified):
        subprocess.run(ostree(repo, "init", "--mode=archive"), check=True)
    subprocess.run(["flatpak", "build-import-bundle", "--no-update-summary",
                    str(imported), str(bundle)], check=True)
    subprocess.run(ostree(
        verified, "remote", "add", f"--gpg-import={public_key}", "karin",
        imported.as_uri(),
    ), check=True)
    subprocess.run(ostree(verified, "pull-local", "--untrusted", "--gpg-verify",
                         "--remote=karin", str(imported), APP_REF), check=True)
    if capture(ostree(verified, "rev-parse", f"karin:{APP_REF}")) != commit:
        raise ValueError("Signed bundle does not contain the verified deployment")


def write_remote_files(packages: Path, repo: Path, name: str, public_key: bytes) -> None:
    pin = base64.b64encode(public_key).decode("ascii")
    reference = packages / f"{name}.flatpakref"
    reference.write_text(
        f"[Flatpak Ref]\nName={APP_ID}\nBranch=master\nTitle=Karin\n"
        f"Url={CHANNEL_URL}\nGPGKey={pin}\nSuggestRemoteName=karin\n"
        f"RuntimeRepo={RUNTIME_REPO}\nIsRuntime=false\n",
        encoding="utf-8",
    )
    remote = packages / "karin.flatpakrepo"
    remote.write_text(
        f"[Flatpak Repo]\nTitle=Karin\nUrl={CHANNEL_URL}\nGPGKey={pin}\n"
        "DefaultBranch=master\n",
        encoding="utf-8",
    )
    shutil.copyfile(reference, repo / "karin.flatpakref")
    shutil.copyfile(remote, repo / remote.name)


def prepare(repository: Path, revision: str, artifacts: Path, output: Path, keyring: Path) -> None:
    build = identity(repository, revision)
    if not re.fullmatch(STABLE_VERSION, build["version"]):
        raise ValueError("The stable channel only accepts a plain SemVer release")
    if output.exists() or output.is_symlink():
        raise ValueError("Use a new channel output directory")
    fingerprint = (repository / PIN).read_text(encoding="ascii").strip()
    key = trusted_key(keyring, fingerprint)
    name = artifact_name(build)
    receipt = json.loads((artifacts / f"{name}-flatpak.BUILD-INFO.json").read_text(encoding="utf-8"))
    export = artifacts / "flatpak-repository"
    with tempfile.TemporaryDirectory(prefix="karin-verify-flatpak-") as temporary:
        work = Path(temporary)
        commit = validate_export(export, receipt, build, work)
        public_key = work / "karin.gpg"
        public_key.write_bytes(key)

        repo = output / "site" / "flatpak"
        packages = output / "packages"
        repo.mkdir(parents=True)
        packages.mkdir()
        subprocess.run(ostree(repo, "init", "--mode=archive"), check=True)
        # Copy only the checked app ref, depth 0. Do not include the build tree,
        # remote credentials, previous deployments, private keys or extra refs.
        subprocess.run(ostree(repo, "pull-local", "--untrusted", str(export), APP_REF), check=True)
        subprocess.run(ostree(repo, "gpg-sign", f"--gpg-homedir={keyring}", commit, fingerprint), check=True)
        subprocess.run([
            "flatpak", "build-update-repo", "--title=Karin", "--default-branch=master",
            f"--gpg-import={public_key}", f"--gpg-sign={fingerprint}",
            f"--gpg-homedir={keyring}", str(repo),
        ], check=True)
        subprocess.run(ostree(repo, "fsck"), check=True)
        verify_signed_pull(repo, work, public_key, commit)

        bundle = packages / f"{name}.flatpak"
        subprocess.run([
            "flatpak", "build-bundle", "--arch=x86_64", f"--repo-url={CHANNEL_URL}",
            f"--runtime-repo={RUNTIME_REPO}", f"--gpg-keys={public_key}", str(repo),
            str(bundle), APP_ID, "master",
        ], check=True)
        verify_signed_bundle(bundle, work, public_key, commit)
        write_remote_files(packages, repo, name, key)
        record = {
            "schema": 1,
            "app_id": APP_ID,
            "ref": APP_REF,
            "version": build["version"],
            "source_commit": revision,
            "ostree_commit": commit,
            "key_fingerprint": fingerprint,
        }
        write_json(packages / UPDATE_RECORD, record)
        shutil.copyfile(packages / UPDATE_RECORD, repo / UPDATE_RECORD)
        installed = {**receipt,
                     "publication": "Signed channel artifacts prepared for owner approval",
                     "stable_channel": record, "remote_url": CHANNEL_URL}
        write_json(packages / f"{name}-flatpak.BUILD-INFO.json", installed)

        size = 0
        for path in (output / "site").rglob("*"):
            if path.is_symlink() or not (path.is_file() or path.is_dir()):
                raise ValueError("Pages snapshot cannot contain links or special files")
            if path.is_file():
                size += path.stat().st_size
        if size > SITE_LIMIT:
            raise ValueError("Stable update snapshot exceeds the Karin Pages budget")
    identity(repository, revision)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--artifacts", type=Path, required=True, help="Output of artifacts.py flatpak")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--keyring", type=Path, required=True, help="Isolated GnuPG home with the pinned signing subkey")
    args = parser.parse_args()
    try:
        prepare(REPOSITORY, args.revision, args.artifacts.resolve(strict=True),
                args.output.absolute(), args.keyring.resolve(strict=True))
    except (OSError, ValueError, KeyError, configparser.Error, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Stable Flatpak channel preparation failed: {error}\n")


if __name__ == "__main__":
    main()
