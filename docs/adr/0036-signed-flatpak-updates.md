<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# 0036: Stable updates through Flatpak

Karin checks the public GitHub latest stable release on explicit user request.
Its independent client admits bounded JSON and plain-text notes without account
credentials. Semantic versions determine availability. Native installations
offer the verified release page because an extracted tarball has no managed
installation contract.

An installable Flatpak update requires both a stable GitHub release and an exact
deployment reported for Karin by the host Flatpak UpdateMonitor. The
`KARIN-FLATPAK-UPDATE.json` asset binds the release version, reviewed Git commit,
app ref, OSTree checksum and pinned signing fingerprint. A different or pending
Portal commit stays unavailable for in-app installation. Flatpak owns the remote,
signature verification, data retention, permission changes and deployment. The
Portal has no API to inspect the origin key or pin a commit as an Update argument.
Verify the installed commit after the transaction, and verify the new process
before completing restart. If the origin or permitted deployment cannot be
confirmed, the user can use the system application manager.

The repository is prepared as a fresh x86_64 snapshot for GitHub Pages. The
release pipeline checks the exported ref and actual installed binary against the
immutable source identity, then signs the deployment and summary. A separate
pull with only the exported pinned public key verifies both signatures and
content. Installation references and the standalone bundle carry this same key
and HTTPS origin. Only explicitly gated stable releases can supply that channel.
Previous versions remain in their own GitHub Releases. The live snapshot ships
one release, with loose objects and no retained static deltas.

The owner must provision the production signing fingerprint and protected
workflow environments before release. See [publication and migration instructions](../BUILDING.md#stable-flatpak-channel).
The deployment keeps the existing application ID, app-data paths and sandbox.
