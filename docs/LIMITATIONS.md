<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Known limitations

The owner verified Karin 0.8.3 on Fedora with KDE Plasma and Wayland, including
sign-in, playback and Discord Rich Presence. The Discord check also passed in
Flatpak. Cloud fixtures cover synthetic service responses and virtual desktop
behavior. The next release provides the first production transition to verify
through the installed 0.8.3 updater.

## Service and distribution

Karin is unofficial. Music API schemas, account access, catalog availability and
service restrictions may change. Only supported unprotected MP3 delivery is
admitted; unknown origins/formats/protection fail explicitly. No DRM bypass,
imported official-client sessions or extracted keys are provided.

Source licensing, reuse of the published OAuth registration and service/content
rights are separate. OAuth and Music integration conditions remain tracked in
[#5](https://github.com/tracerxbrhd/karin/issues/5). See [LICENSING](LICENSING.md).
For Flathub submission requirements, see [release packaging](BUILDING.md#pinned-release-sources).

Lyrics availability/matching depend on the provider. Yandex plain text has no
timing; external LRCLIB lookup needs consent and text remains memory-only.
Software licensing does not grant redistribution rights to service material.

## Library, playback and storage

- Ordinary views stop at 2,000 positions/100 pages. Eligible search-driven
  collection indexes admit 20,000 positions/4 MiB; playback contexts contain
  at most 500 occurrences. Incomplete/changed/oversized coverage is explicit.
- Playlist edits require complete bounded snapshots and readback. Metadata/delete
  lack proven server revision CAS; external-writer races cannot all be excluded.
  Ambiguous writes are not replayed automatically.
- Wave starts explicitly and never restores a radio session. Future-order edits
  are unavailable for Wave. Feedback failures retain honest error/recovery states.
- Audio reuse needs current online Music access and fresh HTTP reuse proof.
  Stored means local presence, not a guaranteed hit or offline right. Offline
  Mode, downloads and mass prefetch are not implemented. Cache failure preserves
  admitted playback; Unlimited still requires disk headroom and bounded work.
- Protected persistence depends on Secret Service/Secret Portal. Missing/locked
  storage permits memory-only login. Restoration needs fresh Music validation;
  no automatic token refresh or remote revocation is implemented. Failed forgetting
  remains visible with Retry; ordinary continuation never autoplays.
- Native plugin calls cannot always be preempted; forced termination cannot
  guarantee cooperative pipeline/storage cleanup.
- If track covers are missing on Fedora ([#37](https://github.com/tracerxbrhd/karin/issues/37)),
  exit Karin and reopen it with `./run-karin --artwork-diagnostic` or
  `flatpak run io.github.tracerxbrhd.Karin --artwork-diagnostic`. Open a view
  with missing covers and share only the `Karin artwork: event=` lines.

## Application updates

- Normal release checks work before Music sign-in. Native packages offer release
  information and the verified download page. Replacement of extracted binaries
  or distribution-managed system packages remains a manual operation.
- Flatpak requires Karin's signed origin and the host Flatpak UpdateMonitor.
  Release announcement, Pages publication and Portal discovery may arrive at
  different times. The host schedules its own checks. Karin waits for the exact
  deployment and can repeat its release/metadata check on request.
- Permission changes or unsupported Portal behavior can require the system
  application manager. The upgrade from 0.8.2 to 0.8.3 uses a system update to
  confirm Discord IPC permissions and keeps the existing App ID and application
  data. In-app restart requires the installed commit to match the confirmed
  deployment. [Update and migration instructions](BUILDING.md#move-an-existing-bundle-installation-to-the-channel)

## Discord

Rich Presence is opt-in and uses the local desktop client's IPC. Presentation
depends on the running Discord client and its activity privacy settings.
Flatpak requires the scoped socket export to be present in its sandbox. A
Native Discord socket created or replaced after Flatpak Karin started can
require relaunching Karin. [Integration contract](adr/0038-local-discord-rich-presence.md)

## Desktop and rendering

System follows Libadwaita preferences, not Qt/Breeze CSS. Karin Style is not yet
available. An external legacy GTK dark-theme setting may cause an initialization
warning; [the optional read-only check](BUILDING.md#theme-input-check) changes no settings.

[#23](https://github.com/tracerxbrhd/karin/issues/23) records substantial cold
software-GL time in Mesa llvmpipe/LLVM draw-variant JIT, with shorter Cairo and
repeated-Play controls. Remaining costs are not fully split; the boundary has not
been accepted for physical Fedora/Wayland/GPU. There is no production prewarm or
forced renderer workaround. A [bounded desktop diagnostic](BUILDING.md#bounded-cold-render-diagnostic)
can check the target platform without repeating accepted v0.8 functionality.
