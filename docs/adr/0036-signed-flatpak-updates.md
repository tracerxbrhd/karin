<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# 0036: Stable updates through Flatpak

On request, Karin reads the public GitHub latest stable release without account
credentials. The release provides the semantic version and notes. Native offers
the verified release page for a manual replacement. Flatpak also resolves the
stable tag to a source commit and reads a bounded schema 1 deployment record
from the pinned HTTPS Pages channel.

The GitHub release notes carry one hidden SHA-256 line for the exact Pages
record. This retains the independent release-to-deployment binding previously
provided by the GitHub asset digest. The record binds the release version, Git
commit, app ID, ref, OSTree commit and pinned signing fingerprint. If the Pages
record, release or Portal offer refer to different deployments, in-app
installation waits. The checksum line is omitted from the notes shown in Karin.

Flatpak verifies the repository with the channel's trusted signing key. Karin
requires the host UpdateMonitor to report the exact offered commit before
installation. The Portal cannot pin a commit in its Update call or expose the
origin key for inspection. Karin checks the installed commit after the
transaction and checks the new process on restart. The metadata identifies
which deployment Karin may request, while Flatpak authenticates the content.

The app retains its UpdateMonitor independently of each dialog's metadata
request. Portal notifications can arrive while release checks are still
running. Flatpak schedules the first and subsequent checks itself (the
upstream default is 30 minutes, with metered networks skipped by default).
The public interface provides no trigger for an immediate poll. The dialog
distinguishes a matching offer, a pending or stale report, an unavailable
Portal and a deployment that the system manager has already installed.
The Update method can include runtime work and refuses an update that adds
sandbox permissions. Show the system update route for such a refusal.
[Flatpak Portal interface](https://docs.flatpak.org/en/latest/portal-api-reference.html)

The release pipeline checks the Native and Flatpak builds against one reviewed
source commit, then signs the Flatpak ref and summary. An independent pull
verifies the signed export with the pinned public key. The bundle carries that
key, the existing HTTPS origin, and the runtime repository. The installation
keeps the application ID, branch and data path. Pages hosts
the signed repository and its deployment record as a bounded current snapshot.

The 0.8.2 GitHub Release includes the same schema 1 record as an attachment for
0.8.1. Later releases attach only the signed Flatpak bundle and Native archive.
An installation on the existing channel that skips 0.8.2 can use the system
Flatpak manager. GitHub automatically generates its two tagged source downloads.

The complete corresponding source archive has a separate public versioned
release in `tracerxbrhd/karin-source`. Main release notes link directly to it.
The publication workflow checks the stable tag, exact public asset set and
digests, notes checksum and anonymous source archive download before Pages
deployment. Owners publish the binary release and approve production Pages
through the existing protected environments. [Publication procedure](../BUILDING.md#stable-flatpak-channel)
