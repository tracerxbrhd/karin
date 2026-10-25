<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Known limitations

- **Yandex Music access remains blocked.** The anonymous status diagnostic does
  not establish a user identity, subscription or stream entitlement. General
  Yandex ID OAuth/PKCE documentation does not establish a Music-specific grant
  for Karin's own application. The explicit `AuthenticationUnavailable`
  capability keeps sign-in unavailable; no borrowed client identity, token-paste
  login, passwords or service credentials are included. See the decision and
  authorized next actions in [API_FEASIBILITY.md](API_FEASIBILITY.md).
- **HTTP restrictions retain their scope.** Anonymous search and metadata
  requests returned HTTP 451 from the cloud egress. That observation does not
  determine authenticated behavior or the cause on Fedora. No alternate region,
  identity, endpoint or delivery path is used to bypass the restriction.
- **The player engine is not a service-stream implementation.** Gate A uses
  real GStreamer `playbin` playback for local files. The developer entry point
  generates temporary quiet WAV media; it is not a song catalog, playlist or
  authenticated Music view. Service stream resolution, library/search/My Wave,
  caching and offline service music remain unimplemented.
  General local media must be trusted, since playlist/decoder plugins can refer
  to other resources. The current GTK entry point and CI use only generated PCM
  WAV; there is no arbitrary-file chooser or playlist import.
- **Silent playback does not prove physical output or every codec.** `fakesink`
  exercises real local-file decoding, transport and EOS without a sound device.
  It does not establish PipeWire audibility, codec support for an unknown future
  stream, or a permitted Yandex playback path. The user-reported v0.0 audible
  diagnostic on Fedora is distinct from Gate A's pending system-output tests.
- **Desktop acceptance remains manual.** Issue #3 reports that the v0.0 native
  and Flatpak prototypes launch and diagnostic audio works on Fedora 44 / KDE
  Plasma / Wayland / PipeWire. Gate A's controls, output, resize behavior,
  keyboard/focus, HiDPI and accessibility need fresh acceptance there. The earlier
  visual rejection remains historical evidence; startup reports and Debian/Xvfb
  screenshots do not establish subsequent visual signoff.
- **Shutdown is cooperative.** Playback commands and shutdown requests are
  asynchronous. The worker observes cancellation and resets/drops its pipeline
  before reporting completion; dropping a handle alone is not proof of cleanup.
  Existing HTTP/finite diagnostics may hold the application until their bounded
  work completes. Forceful process termination cannot guarantee cleanup.
- **Secret Service and MPRIS are not completed integrations.** The application
  inspects session bus/keyring presence without reading secrets. It does not
  store credentials or publish a player on MPRIS. Those integrations remain
  separate future work.
- **Gate A packaging acceptance remains pending.** The v0.0 Flatpak startup
  result is user-reported. Source preparation and manifest parsing in this cloud
  do not establish a fresh sandboxed build/install of Gate A; Bubblewrap user
  namespaces are denied in the nested container. GNOME 50, the Freedesktop 25.08
  Rust extension and the existing permissions are retained.
- **Verification reports are environment-specific.** Executed local checks and
  remaining manual gates are recorded in [VALIDATION.md](VALIDATION.md). The
  actual GitHub Actions run is authoritative for hosted-runner outcomes; adding
  a workflow step alone does not establish that it passed.
- **Licensing does not grant service access.** Karin's own code and original
  materials use GPL-3.0-or-later; third-party components retain their terms.
  Distribution still requires corresponding source/notices and applicable
  component obligations. GPL does not override Yandex Music terms or authorize
  API use or redistribution of service content.
