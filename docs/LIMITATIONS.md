<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Known limitations

- **No service login or music playback.** The anonymous status endpoint has been
  probed, but this does not verify a user identity, subscription or streaming.
  Search and metadata probes returned HTTP 451 from this cloud egress. See
  [API_FEASIBILITY.md](API_FEASIBILITY.md) for feature-specific evidence.
- **No permitted authentication mechanism is established for Karin.** No Yandex
  client registration, client secrets, user tokens, DRM workaround or borrowed
  OAuth identity is included. Legal/service permission remains unresolved.
- **Audio is diagnostic only.** `fakesink` verifies processing to EOS; it cannot
  prove audibility or codec support for service streams. The user has separately
  confirmed an audible synthetic test signal on Fedora 44/KDE/Wayland/PipeWire;
  this establishes that reported test, not Yandex Music playback.
- **Revised visual acceptance remains manual.** The user confirmed functional
  launch, anonymous API HTTP 200, audio diagnostics, D-Bus and Secret Service on
  Fedora 44/KDE/Wayland/PipeWire, but rejected the original visual layout.
  The revised layout must undergo repeat manual acceptance there. Debian/Xvfb
  screenshots do not establish KDE window-decoration behavior, HiDPI,
  screen-reader usability or a completed accessibility audit.
- **Secret Service and MPRIS are boundaries, not completed integrations.** The
  application can inspect session bus/keyring presence, but never reads secrets
  and does not advertise a controllable music player.
- **Normal shutdown waits for bounded work.** Active diagnostics hold the process
  alive until cleanup, up to the network/audio timeout. Forceful termination can
  interrupt that cleanup.
- **Flatpak packaging is a foundation.** Manifest parsing and source preparation
  can be checked here; a full sandboxed build is blocked by denied user namespaces
  in the nested cloud container. No packaged application was validated.
- **CI configuration is not a remote CI result.** Local commands are reported
  separately in [VALIDATION.md](VALIDATION.md). GitHub's run is authoritative for
  hosted-runner results.
- **Licensing does not grant service access.** Karin's own code and original
  materials use GPL-3.0-or-later; third-party components retain their terms.
  Distribution still requires the corresponding source/notices and any
  component-specific obligations. GPL does not override Yandex Music terms,
  provide an API permission or authorize redistribution of service content.
