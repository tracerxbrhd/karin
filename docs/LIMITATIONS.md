# Known limitations

- **No service login or music playback.** The anonymous status endpoint has been
  probed, but this does not verify a user identity, subscription or streaming.
  Search and metadata probes returned HTTP 451 from this cloud egress. See
  [API_FEASIBILITY.md](API_FEASIBILITY.md) for feature-specific evidence.
- **No permitted authentication mechanism is established for Karin.** No Yandex
  client registration, client secrets, user tokens, DRM workaround or borrowed
  OAuth identity is included. Legal/service permission remains unresolved.
- **Audio is diagnostic only.** `fakesink` verifies processing to EOS; it cannot
  prove audibility, codec support for service streams or PipeWire integration.
- **Desktop acceptance remains manual.** Debian/Xvfb is not Fedora 44 KDE Plasma
  on Wayland. HiDPI, screen-reader usability and real audio output require testing
  on the target desktop. The native layout supports keyboard focus and narrow
  windows but is not a completed accessibility audit.
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
- **License remains undecided.** A project license must be selected before the
  first public release. No release or merge is performed by this implementation.
