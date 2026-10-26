<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Known limitations

- **Yandex Music authorization remains unverified.** On 2026-10-26 the owner
  confirmed Karin's own user-authorization registration with two Music scopes
  and two ID scopes. This resolves the registration uncertainty, not token
  issuance, endpoint rights, account entitlement or stream delivery. Gate B1's
  opt-in local OAuth experiment has not been exercised with an account on Fedora.
  The GUI keeps `AuthenticationUnavailable`; no borrowed client, pasted-token
  login, password collection or service stream is included. See
  [API_FEASIBILITY.md](API_FEASIBILITY.md) and [ADR-0008](adr/0008-local-oauth-pkce-diagnostic.md).
- **The OAuth diagnostic is not a production login/session.** Its documented
  verification-code page is for manual developer testing. With code-only entry,
  no returned state is available for an honest callback-state comparison; the
  tool sends random state and relies on S256 PKCE for code binding, reporting
  that limitation. The initial exchange is secretless, but public native Linux
  callback, refresh/revocation and secure permanent storage remain unverified.
  Optional token-scope metadata is not independent Music entitlement evidence.
  Tokens are memory-only, no stream is requested and account tests must remain
  local to the consenting owner.
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
