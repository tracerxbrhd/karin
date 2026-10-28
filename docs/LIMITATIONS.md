<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Known limitations

- **Yandex Music access remains forbidden in the tested flow.** On 2026-10-26
  the owner
  confirmed Karin's own user-authorization registration with two Music scopes
  and two ID scopes. Registration alone establishes neither endpoint rights,
  account entitlement nor stream delivery. The owner
  subsequently observed Karin's browser consent and an opaque code with returned
  state on Fedora. After the initial code-format correction, a fresh local run
  validated state, received a token and verified the Karin client through
  Yandex ID (both HTTP 200). Authenticated Music status returned HTTP 403 again
  in the DIRECT-path control; account/service facts remain unknown and the
  catalog query is skipped. Anonymous curl status HTTP 200 does not change that.
  The new independent `/account/about` probe has no live local result yet. Its
  success would not establish status account/service facts or unlock the catalog;
  its failure does not suppress catalog access after valid authenticated status.
  The GUI keeps `AuthenticationUnavailable`; no borrowed client, pasted-token
  login, password collection or service stream is included. See
  [API_FEASIBILITY.md](API_FEASIBILITY.md) and [ADR-0008](adr/0008-local-oauth-pkce-diagnostic.md).
- **The OAuth diagnostic is not a production login/session.** Its documented
  verification-code page is for manual developer testing. The hidden prompt
  accepts the full returned browser URL and validates state before extracting
  the opaque code; a code-only fallback is not offered. This manual transfer is
  not an automatic callback or production Linux login. The initial exchange is
  secretless, but public native Linux callback, refresh/revocation and secure
  permanent storage remain unverified.
  Optional token-scope metadata is not independent Music entitlement evidence.
  Tokens are memory-only, no stream is requested and account tests must remain
  local to the consenting owner. After successful ID-client/account verification,
  status and about run independently with separate sanitized HTTP/error results.
  About validates only a bounded JSON object `result`, without top-level `error`,
  and discards all fields; it is not an established personal-account model.
- **HTTP restrictions retain their scope.** Anonymous search and metadata
  requests returned HTTP 451 from the cloud egress. That observation does not
  determine authenticated behavior or the cause on Fedora. The later Fedora
  DIRECT-path and anonymous controls weaken a simple proxy/TUN explanation but
  do not prove identical egress or identify the reason for Music HTTP 403.
  Only exact `missing-required-scopes` in a string `error` or nested `error.name`
  is classified; the actual Fedora response's provider identifier has not been
  observed here. Unknown identifiers and unsupported bodies remain `Forbidden`.
  Raw messages and
  private response data are never exposed. No alternate route, region,
  identity or delivery path is used to bypass the restriction. The separately
  planned same-token about probe is not a fallback for refused status data.
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
