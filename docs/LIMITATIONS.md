<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Known limitations

- **Live Karin sign-in/library acceptance is pending.** The default provider now
  adapts Cassette's published WebKit session → OAuth sessionid chain. It is a real
  attempt, without developer fields, code copying, token paste or app registration.
  It is not the known own-client PKCE route, which previously returned ID 200 and
  Music status/about403 with unknown cause. Cloud fixtures do not prove a real
  login, Music account or personal library/search; run [the single Fedora check](BUILDING.md#music-sign-in-and-fedora-acceptance).
- **Return/session/token outcomes are concrete stages.** Exact Music-root return
  and fresh echoed state must be accepted before collecting OAuth-endpoint cookies
  from this dialog's ephemeral session. A different return, missing state/cookie,
  cookie-read failure, OAuth HTTP/schema refusal and Music API refusal are distinct
  errors, not a generic provider-unavailable result. Sessionid is not Code+PKCE;
  the independent own-client PKCE diagnostic remains unchanged.
- **Service/identity distribution conditions remain unresolved.** The auth
  adaptation and published shared registration are attributed under Cassette's
  GPL-3.0-or-later terms; code licensing does not grant API, registration, content
  or trademark rights. Express approval for Karin's reuse/public distribution
  and Linux embedded auth is not established. Do not infer a blanket provider ban
  or approval. RFC 8252 rejects embedded native agents; mobile LoginSDK WebView
  support does not resolve Linux eligibility. See [the scoped decision](adr/0011-cassette-session-auth-prototype.md).
- **Device Flow is a library-only experiment.** `experimental-device-auth` remains
  excluded from default/Flatpak and has no GUI or bundled app identity. The normal
  default/Flatpak instead enables `cassette-auth`/WebKit. A no-feature build omits
  real auth intentionally; smoke/capture modes use an explicit offline provider
  and must not be mistaken for the real Fedora acceptance build.
- **Sessions do not survive restart.** Tokens remain in memory; no plaintext
  credential file, environment input or token store is used. Logout/close clears
  local session state, but does not
  revoke Yandex's server grant. Refresh, remote revocation and a verified Secret
  Service lifecycle remain future gates. Keyring presence detection alone does
  not complete them.
- **Music data acceptance is endpoint-specific.** The adapter and UI implement
  read-only account/library/playlist/search/details requests and paging, but must
  be checked on Fedora with an authorized test session. Prior owner evidence
  covers yamu account/search/track/album, not every Karin endpoint or schema.
  Login completion and actual library loads require Music account validation;
  neither browser navigation nor token issuance supplies that acceptance.
  Writes, like/unlike, playlist editing, recommendations and My Wave are excluded
  until their methods and permissions are independently validated.
- **Real Music audio is blocked.** The reviewed yamu delivery path requires
  extracted official-client signing material and non-Karin request identity;
  a permitted unsigned, identity-neutral source contract has not been established.
  No signing key, protected-stream decryption, audio URL resolver or service
  download is implemented. The metadata queue can be edited, but does not pretend
  to play tracks. See [delivery evidence](STREAM_DELIVERY_REVIEW.md).
- **The GStreamer engine remains independent.** Local-file playback and clearly
  labeled generated-WAV diagnostics exercise decoding, transport, seeking, EOS
  and cleanup. They do not establish service playback, every future codec or
  physical PipeWire output. MPRIS remains deferred until a real service source
  and truthful playback state exist.
- **Cloud results are limited.** Synthetic loopback fixtures and Xvfb/Openbox
  captures establish only their stated protocol/UI behavior. They cannot certify
  Fedora/KDE/Wayland decorations, HiDPI, accessibility, account rights, audible
  music or the exact Flatpak artifact. Fresh owner acceptance remains required.
- **Shutdown is cooperative.** Cancel/logout discards stale work and local
  presentation state; a bounded in-flight HTTP call can retain a temporary token
  clone until it completes. Forceful process termination cannot guarantee native
  cleanup or erase all HTTP/TLS/OS copies. No raw provider diagnostics or personal
  data should be published from local tests.
- **Flatpak acceptance is pending for this revision.** GNOME 50, Freedesktop
  25.08 Rust extension and existing permissions are preserved. No host filesystem
  or Secret Service grant is added. Source preparation and offline Cargo checks
  do not establish a full sandboxed build; Bubblewrap namespaces are denied in
  the cloud environment.
- **Code licenses do not grant service access.** GPL-3.0-or-later and compatible
  dependency licenses require their notices/source obligations and grant no
  Yandex API, content, application-identity or trademark rights. The thin adapter
  uses schema knowledge from pinned yamu source without importing its OAuth or
  delivery implementation. The separate Cassette auth adaptation retains its own
  GPL notices and shared-registration provenance; service rights remain separate.

Executed checks and outstanding acceptance are recorded in
[VALIDATION.md](VALIDATION.md). A workflow definition is not a passed run.
