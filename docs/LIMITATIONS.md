<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Known limitations

- **Core service audio is accepted for the recorded baseline.** The owner's
  [Fedora run at `8f1b09d`](https://github.com/tracerxbrhd/karin/pull/13#issuecomment-6539311586)
  confirms an audible entitled Yandex Music track, working controls and queue,
  and stopping on logout/close. This is real service acceptance, not a generated
  signal. The later `f0aa2bb` run reports Search PASS, Account PASS and preserved
  Playback PASS, but UI FAIL. The next workspace/context/MPRIS increment needs
  fresh acceptance. It does not establish every format/track, a current Flatpak
  artifact or public registration/service-distribution permission.
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
- **Track normalization is accepted, with explicit bounds.** The historical `8f1b09d` result was HTTP
  200 / `track-wire` / `track-fields-unsupported`; own ID validation succeeded,
  but remaining display/wire metadata could not deserialize. No private response
  or exact culprit field was supplied. The correction reads minimal validated
  search IDs, accepts at most 50 raw hit positions and normalizes one batch of
  at most 50 distinct IDs through the
  working account-bound `/tracks` lookup. It preserves order/composite IDs,
  duplicate positions and provider pagination, reports missing/null records and
  keeps unavailable returned tracks disabled. Oversized pages fail before POST,
  without N+1, truncation or a synthetic cursor. The owner's corrected-search
  PASS at `f0aa2bb` supersedes that failure; the overall UI was rejected. New
  unified tracks/albums/artists/playlists search and category paging need their
  own live checks. Search playlist summaries preserve actual owner/kind and
  leave an absent track count unknown.
- **Playback context is the loaded collection, not an unlimited library.** A
  row activation starts its exact occurrence in the returned collection, within
  the 500-position queue bound. Order and duplicates are retained; unavailable
  loaded items are skipped by Next/Previous/EOS. Partial contexts stay marked,
  with no unbounded autofetch. Manual appends follow the loaded context; choosing
  a new context replaces old extras. These new semantics require owner acceptance
  without invalidating the previously accepted core sound.
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
  without disclosing response data. That historical failure is not a current
  claim that audio or all details remain broken; use the latest revision-specific
  owner evidence above. The later real `f0aa2bb` search PASS supersedes that
  search failure; green fixtures alone could not have established it.
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
- **The GStreamer engine remains independent.** Local-file playback and clearly
  labeled generated-WAV diagnostics exercise decoding, transport, seeking, EOS
  and cleanup. They do not establish service playback, every future codec or
  physical PipeWire output. The new MPRIS adapter uses the existing player,
  without another audio engine or arbitrary-URI input. Native display metadata
  and an optional validated local cover are exposed on the user's session bus,
  never media/CDN addresses, tokens or cookies. Actual KDE/playerctl/hardware-key
  and current Flatpak integration still require owner-local validation; repeat,
  shuffle, TrackList and variable-speed playback are not implemented. Optional
  LoopStatus/Shuffle properties are omitted. The worker adds only successful-
  seek acknowledgement telemetry for the bridge; source resolution, admission
  and the underlying media algorithms are preserved.
- **Cloud results are limited.** Synthetic loopback fixtures and Xvfb/Openbox
  captures establish only their stated protocol/UI behavior. They cannot certify
  Fedora/KDE/Wayland decorations, HiDPI, accessibility, account rights, audible
  music or the exact Flatpak artifact. Fresh acceptance of each changed
  workflow/revision remains required; this does not erase the separately
  recorded real audio PASS.
- **Shutdown is cooperative.** Cancel/logout discards stale work and local
  presentation state; a bounded in-flight HTTP call can retain a temporary token
  clone until it completes. Forceful process termination cannot guarantee native
  cleanup or erase all HTTP/TLS/OS copies. No raw provider diagnostics or personal
  data should be published from local tests.
- **Flatpak acceptance is pending for this revision.** GNOME 50, Freedesktop
  25.08 Rust extension are preserved. MPRIS adds only ownership of
  `org.mpris.MediaPlayer2.io.github.tracerxbrhd.Karin`, without broad session-bus,
  host filesystem or Secret Service grants. Source preparation and offline Cargo checks
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
