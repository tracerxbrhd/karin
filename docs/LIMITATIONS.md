<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Known limitations

- **Public Music sign-in is not established.** The owner verified Karin-owned
  PKCE/token issuance and Yandex ID, but both Music `/account/status` and
  `/account/about` returned generic HTTP 403 with TUN on and off. Their exact
  cause remains unknown; selected scopes, ID success and anonymous HTTP 200 do
  not establish Music entitlement. See [API evidence](API_FEASIBILITY.md).
- **Experimental Device Flow is local and opt-in.** It requires the build feature
  and runtime flag, and locally entered application credentials the owner is
  entitled to test. No official-app defaults or credentials are bundled. The
  successful unmodified yamu experiment does not authorize Karin distribution
  under another application's identity. The default/public Flatpak excludes
  this flow. No password, cookie scraping or token-paste fallback is provided.
- **Sessions do not survive restart.** Tokens and locally entered application
  credentials remain in memory; no plaintext credential file, environment input
  or token store is used. Logout/close clears local session state, but does not
  revoke Yandex's server grant. Refresh, remote revocation and a verified Secret
  Service lifecycle remain future gates. Keyring presence detection alone does
  not complete them.
- **Music data acceptance is endpoint-specific.** The new adapter and UI implement
  read-only account/library/playlist/search/details requests and paging, but must
  be checked on Fedora with an authorized test session. Prior owner evidence
  covers yamu account/search/track/album, not every Karin endpoint or schema.
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
  uses schema knowledge from pinned yamu source without importing its OAuth,
  official-client identity or delivery implementation.

Executed checks and outstanding acceptance are recorded in
[VALIDATION.md](VALIDATION.md). A workflow definition is not a passed run.
