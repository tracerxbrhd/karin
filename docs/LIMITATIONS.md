<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Known limitations

- **The remote player is implemented but service audio is not accepted yet.**
  `StreamResolver` now handles the guarded legacy full-MP3 offer/XML path and
  gives an opaque short-lived HTTPS source to the existing GStreamer worker.
  Play/pause/resume/Stop, progress/seek, volume, queue navigation and EOS have
  code paths; executed checks are recorded separately in [validation](VALIDATION.md).
  The owner has not yet supplied a live descriptor or heard service track.
  The 60-second local admission starts before the offer request, rather than
  treating XML `ts` as an expiry grant; a playing track is not stopped merely
  when that admission ages out. Reconnection/seek may need an explicit fresh Play.
- **Remote delivery support is deliberately narrow.** Metadata accepts only
  exact HTTPS `storage.mds.yandex.net` with `/get-download-info/`,
  `/download-info/` or `/file-download-info/`. Media accepts the restricted generated
  `s` + 1–5 digits + 3 lowercase letters host family under `storage.yandex.net`
  and canonical `/get-mp3/` resources, not arbitrary Yandex subdomains. This is
  a source-backed policy, not an exhaustive observed CDN inventory. Missing
  full-track preview permission, protection markers, unsupported codecs,
  ambiguous metadata, unknown hosts, redirects and expiry fail visibly. No
  official extracted signer, DRM decryption, media cache or fallback identity
  is included. See [ADR-0015](adr/0015-guarded-legacy-mp3-playback.md).
- **The later `99a177e` Fedora track-search retest also failed.** Ordinary login,
  the library, track details/liked state and album search/details worked. The
  subsequent correction handles source-backed mixed/null optional display
  credits and exposes finite schema-shape support codes, with a native failing-
  before/passing-after regression. It is not a claim to have seen the owner's
  payload or passed another real-service test. The owner's delivery-info read
  returned HTTP 200/mp3 but stopped before descriptor retrieval. See the latest
  [validation evidence](VALIDATION.md); search and audible Music acceptance are
  separate, and audio work continues in the child PR without waiting for search.
- **Live acceptance is endpoint-specific.** The owner verified ordinary Karin
  sign-in, Music account validation, actual liked tracks and personal-playlist
  listing on Fedora at PR #8 baseline `797ba3a`, as recorded in Issue #9. Search
  and opening those playlists failed. Fixes require new owner-local acceptance;
  successful loopback fixtures do not change the historical failure into a pass.
  The default Cassette-compatible provider remains the working sign-in path;
  the separate own-client PKCE ID 200 / Music 403 result is not reused as its
  default route or diagnosed as the cause of unrelated UI failures.
- **The 2026-10-30 PR #12 catalog retest failed.** Account/library reads still
  work, but track search reports `response-contract`, and actual row activation
  did not open details. The correction replaces the GTK keybinding-signal hooks
  with parent-list user-activation dispatch and accepts source-confirmed missing
  embedded album identity. The precise live search-response mismatch is still
  unknown; fixed parsing stages and the known HTTP status now distinguish it
  without disclosing response data. New owner-local row/search/detail and
  delivery-entry acceptance remains pending; historical green CI is not a pass.
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
- **Metadata support does not establish all workflows.** Search, details,
  paging, cancellations and independent action dispatch are under correction for
  v0.2. Album/artist/playlist schema and HTTP outcomes need separate checks; do not
  loosen account validation or silently hide a malformed response. Like/unlike
  and liked-playlist reads now have implementations and synthetic tests; live
  acceptance remains pending. Unknown write outcomes require a read refresh,
  not automatic replay. Playlist editing, recommendations and My Wave remain
  absent until their full contracts and lifecycle are implemented and checked.
- **Real service audio acceptance is NOT MET.** v0.2 requires audible actual
  library/search music on Fedora. The latest owner result confirms only an
  authenticated MP3 offer and the old metadata-path refusal; no metadata or
  audio GET followed. The new guarded source path must now be exercised using
  the owner's ordinary local session. An ordinary legacy checksum or returned
  signed resource is not automatically DRM, and a GPL implementation does not
  establish service/distribution rights. No extracted signing/decryption keys,
  access-control workaround or copyrighted audio distribution is included.
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
  uses schema knowledge from pinned yamu without importing its OAuth or modern
  extracted-key delivery implementation. The Cassette auth adaptation retains
  its GPL notices and shared-registration provenance. The original Rust legacy
  resolver cites the published Cassette/MarshalX/lifeart protocol references;
  source, registration and service/content rights remain separate.

Executed checks and outstanding acceptance are recorded in
[VALIDATION.md](VALIDATION.md). A workflow definition is not a passed run.
