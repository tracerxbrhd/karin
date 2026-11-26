#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Prepare, verify and package a signed stable Flatpak channel snapshot."""

import argparse
import base64
import configparser
import json
import os
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
RUNTIME_REF = "org.gnome.Platform/x86_64/50"
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
            or parser.get("Application", "runtime") != RUNTIME_REF):
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


def key_fingerprints(key: Path, home: Path) -> list[str]:
    listing = capture(["gpg", "--batch", "--no-options", "--homedir", str(home),
                       "--with-colons", "--show-keys", str(key)])
    lines = listing.splitlines()
    if sum(line.startswith("pub:") for line in lines) != 1:
        raise ValueError("Expected one bundle signing certificate")
    fingerprints = [line.split(":")[9] for line in lines if line.startswith("fpr:")]
    if not fingerprints:
        raise ValueError("Expected a bundle signing fingerprint")
    return fingerprints[:1] + sorted(fingerprints[1:])


def verify_signed_bundle(bundle: Path, work: Path, public_key: Path, commit: str,
                         channel_url: str = CHANNEL_URL) -> dict[str, str]:
    """Require a clean stock installation to trust and track the signed channel."""
    env = os.environ.copy()
    for variable, directory in {
        "FLATPAK_USER_DIR": "bundle-installation",
        "FLATPAK_SYSTEM_DIR": "bundle-empty-system",
        "XDG_CONFIG_HOME": "bundle-config",
        "XDG_DATA_HOME": "bundle-data",
        "XDG_CACHE_HOME": "bundle-cache",
        "XDG_STATE_HOME": "bundle-state",
        "XDG_RUNTIME_DIR": "bundle-runtime",
    }.items():
        path = work / directory
        path.mkdir(mode=0o700)
        env[variable] = str(path)
    # No preinstalled keys or remotes. Flatpak must verify the bundled commit
    # with its embedded key and provision the future update origin. The sign
    # runner has no runtime installed; no deps means this check needs no network.
    # A disposable bus also supports installations in builders without a system bus.
    subprocess.run([
        "dbus-run-session", "--", "sh", "-ec",
        'DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS" exec "$@"', "karin-bundle-check",
        "flatpak", "install", "--user", "--bundle", "--noninteractive", "--no-deps",
        "--no-related", str(bundle),
    ], check=True, env=env)

    def info(option: str) -> str:
        return subprocess.check_output(["flatpak", "info", "--user", option, APP_ID],
                                       text=True, env=env).strip()

    origin = info("--show-origin")
    if not re.fullmatch(r"[a-zA-Z0-9_-]+", origin):
        raise ValueError("The signed bundle did not configure a Flatpak origin")
    remotes = subprocess.check_output(
        ["flatpak", "remote-list", "--user", "--columns=name,url"], text=True, env=env,
    ).splitlines()
    if remotes != [f"{origin}\t{channel_url}"]:
        raise ValueError("The signed bundle does not track the expected channel")
    if info("--show-ref") != APP_REF or info("--show-commit") != commit:
        raise ValueError("The signed bundle installed an unexpected deployment")
    runtime = info("--show-runtime")
    if runtime != RUNTIME_REF:
        raise ValueError("The signed bundle installed an unexpected runtime reference")

    repo = Path(env["FLATPAK_USER_DIR"]) / "repo"
    config = configparser.ConfigParser(interpolation=None)
    config.read_string((repo / "config").read_text(encoding="utf-8"))
    remote = config[f'remote "{origin}"']
    if (not remote.getboolean("gpg-verify", fallback=False)
            or not remote.getboolean("gpg-verify-summary", fallback=False)):
        raise ValueError("The bundle origin must verify the commit and summary signatures")
    local_key = repo / f"{origin}.trustedkeys.gpg"
    gpg_home = work / "bundle-gpg-verification"
    gpg_home.mkdir(mode=0o700)
    expected_keys = key_fingerprints(public_key, gpg_home)
    if key_fingerprints(local_key, gpg_home) != expected_keys:
        raise ValueError("The bundle origin does not trust the pinned signing certificate")
    return {"origin": origin, "url": channel_url, "ref": APP_REF, "commit": commit,
            "runtime": runtime, "key_fingerprint": expected_keys[0]}


def write_remote_files(packages: Path, repo: Path, public_key: bytes) -> None:
    pin = base64.b64encode(public_key).decode("ascii")
    reference = packages / "karin.flatpakref"
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
    shutil.copyfile(reference, repo / reference.name)
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

        bundle = packages / f"{name}-x86_64.flatpak"
        subprocess.run([
            "flatpak", "build-bundle", "--arch=x86_64", f"--repo-url={CHANNEL_URL}",
            f"--runtime-repo={RUNTIME_REPO}", f"--gpg-keys={public_key}", str(repo),
            str(bundle), APP_ID, "master",
        ], check=True)
        bundle_installation = verify_signed_bundle(bundle, work, public_key, commit)
        write_remote_files(packages, repo, key)
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
                     "stable_channel": record, "remote_url": CHANNEL_URL,
                     "bundle_installation": bundle_installation}
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
