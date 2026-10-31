<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Known limitations and acceptance boundaries

## Accepted baseline and new work

The owner accepted the merged v0.2 `master` on Fedora 44 / KDE Plasma / Wayland /
PipeWire, including Flatpak: **17 PASS, 0 FAIL, 1 N/D**. Sign-in, library/search,
actual Music playback, contextual queues, transport/seek/volume, MPRIS/KDE Media
Widget and shutdown form the regression baseline. Hardware media keys are N/D.
PR #13 is merged. Earlier search/UI/audio/packaging failures remain history for
those exact revisions, not the current baseline.

v0.3 is a new protected-session/cache/workspace iteration. Code implementation,
automated checks, actual desktop/backend acceptance and public-release rights
are separate statuses. The baseline PASS does not accept the new revision; only
executed results belong in [VALIDATION.md](VALIDATION.md). Use the
[single Fedora procedure](BUILDING.md#v03-seamless-experience-acceptance-on-fedora).

## Session and system storage

- **Protected storage is an actual capability, not a service-name check.** Native
  Secret Service and Flatpak Secret Portal/backend availability differ by desktop
  and configuration. Locked, denied, cancelled, corrupt, missing or timed-out
  storage permits ordinary process-only sign-in without plaintext fallback.
  Active cleanup I/O settles first; after a failed result, memory-only login can
  continue while restoration/new persistent writes/cache remain barred.
  Synthetic protocol fixtures do not establish a working KWallet/GNOME/portal
  lifecycle or current sandbox support.
- **Only a Music-validated token may be remembered/restored as authenticated.**
  A loaded credential, Yandex ID, previous account or cached library is not fresh
  Music authorization. Restore and validation run off GTK, with generations and
  cancellation. A network failure is not automatically an expired/revoked token.
  Existing Cassette-compatible sign-in remains the normal provider; the own-client
  PKCE ID 200/Music 403 experiment is independent historical evidence.
- **Forget is local and must be confirmed.** Turning remembering off deletes the
  saved credential while the live session may continue. The saved preference
  remains visible when storage is unavailable: enabled may be turned off to
  record forget intent, with unresolved deletion shown; enabling requires
  supported storage. Confirmed logout stops
  audio, clears personal UI and persistent metadata/covers, and verifies saved
  credential absence. Unknown delete/clear outcomes stay visible with Retry;
  replacement login waits for active cleanup I/O, not an invented success.
  Settled failure permits only memory login with persistent writes/cache paused.
  A successfully saved pending-forget barrier suppresses old restoration across
  restart; a failed barrier write leaves restart protection explicitly
  unconfirmed and requires Retry. Normal close may
  retain an allowed cache and confirmed remembered session.
- **Portal and remote lifetimes differ.** Portal forgetting deletes Karin's
  authenticated encrypted token envelope, not the portal's per-app master key.
  Native Secret Service at-rest protection belongs to the configured system
  keyring. No refresh-token or Yandex server-revocation flow is claimed. WebKit
  cookies, sessionid, passwords and diagnostic OAuth inputs remain ephemeral.
- **No broad sandbox assumption.** The exact MPRIS own-name grant does not grant
  direct Secret Service access. Flatpak selects the portal path and an explicit
  unavailable result rather than adding broad session-bus/host-filesystem grants.
  GNOME source-graph libsecret availability is not a test of an installed runtime
  or running portal backend. See [ADR-0018](adr/0018-persistent-session-and-settings.md).

## Cache and offline behavior

- **Metadata and normalized covers only.** The private account-bound disk cache
  has TTL, byte/count limits, atomic writes, integrity/schema validation and
  LRU-like eviction. Hashed paths avoid literal IDs but are not encryption;
  local metadata remains personal material and must not be published. No token,
  cookie, raw provider payload, signed media URL or music belongs there.
- **Saved data is not current authority.** Explicit refresh, account validation,
  favorite membership, playlist revisions and write confirmation use the server.
  Eligible fallback is visibly saved/potentially stale; it must not hide rejected
  authorization or manufacture successful writes. Every write outcome invalidates
  affected metadata, including ambiguous results. Public covers obey explicit
  cache restrictions and shorter source freshness.
- **Disable, clear and logout have different effects.** Cache-off stops persistent
  content reads/new writes while retaining existing files until explicit clear.
  Usage/clear remain administrative operations. Clear preserves credentials,
  current audio/context, displayed textures and live MPRIS cover; account/policy
  generations prevent late repopulation. Logout deliberately stops playback and
  purges personal cache. Normal close retains allowed data. Measured usage sums
  record file lengths, not all physical filesystem allocation overhead.
- **Persistent audio/offline rights are not established.** An authorized legacy
  MP3 stream and official-client encrypted downloads do not supply Karin with a
  third-party persistent-storage grant, account/subscription lease or enforceable
  offline rights-expiry API. No service audio is written to disk or hidden in a
  temporary cache; no inert Offline switch or availability badge is exposed.
  This does not classify every MP3 checksum as DRM or imply the working stream
  failed. See [AUDIO_CACHE_REVIEW.md](AUDIO_CACHE_REVIEW.md) for exact evidence and
  prerequisites, and [ADR-0019](adr/0019-bounded-account-cache.md) for data policy.

## Catalog, workspace and playback

- **The API is unofficial and changeable.** Strict own identifiers/account binding
  and bounded requests remain. Track search accepts at most 50 raw hit positions
  and one canonical batch of at most 50 distinct IDs, preserving duplicate order,
  composite identities and provider paging. Optional hit display fields cannot
  reject otherwise valid IDs; missing records are reported and unavailable
  tracks stay disabled. New contract variants still need explicit handling.
- **A context is a loaded collection.** The 500-position bound, exact occurrences,
  manual additions and visible partial boundary remain. Next/Previous/EOS skip
  unavailable loaded tracks; no unbounded library autofetch or claim of a complete
  remote collection is introduced. Pane/view changes must not replace playback.
- **UI actions require actual data/contracts.** Track-pane artist/album links use
  verified identities, not guessed names. Native playlist tiles permit bounded
  two-line title ellipsis only with full tooltip/accessibility/detail text.
  Creation requires independent server confirmation; ambiguous writes are not
  replayed. Broader playlist editing, repeat/shuffle, TrackList, variable speed,
  notifications and unrelated library features are not implied.
- **My Wave/similar tracks/lyrics have separate gates.** A station list or normal
  queue is not a radio-session/listening-feedback implementation. Keep My Wave
  hidden until its full current session/capability is established. Do not publish
  fabricated recommendations. The reviewed lyrics implementations use extracted
  official-client signing material; Karin imports none, and no separately
  permitted plain-text source is established. See [CASSETTE_PARITY.md](CASSETTE_PARITY.md).
- **Remote delivery remains deliberately narrow.** Metadata admits exact HTTPS
  `storage.mds.yandex.net` and the reviewed `/get-download-info/`, `/download-info/`
  or `/file-download-info/` families. Generated media uses the restricted `s` +
  1–5 digits + 3 lowercase letters host family under `storage.yandex.net`, with
  canonical `/get-mp3/`. This is not arbitrary Yandex subdomains or an exhaustive
  observed CDN inventory. Protected/preview/ambiguous offers, unknown origins,
  redirects and unsupported formats fail visibly; no identity/key/DRM fallback
  is added. The 60-second local admission is not XML timestamp entitlement or
  offline expiry; an already playing track is not stopped just because it ages.
  A new connection/seek/resume may need explicit fresh Play. See
  [ADR-0015](adr/0015-guarded-legacy-mp3-playback.md).
- **MPRIS controls the same player.** Known metadata, opaque track paths and an
  optional private local PNG are exposed on the user's session bus, never media
  URLs, credentials or account IDs. Native seeks require actual worker
  acknowledgement. No second engine or arbitrary OpenUri is introduced.
  Baseline KDE/Flatpak success does not automatically accept new cache/pane/
  lifecycle interactions; physical media keys remain N/D until exercised.

## Evidence and release scope

- **Cloud evidence has limits.** Synthetic loopback/D-Bus/files and Xvfb/Openbox
  prove only their stated protocol, structure, pixels and lifecycle behavior.
  They do not certify Fedora Wayland, real KWallet/portal, actual account rights,
  PipeWire audibility, hardware keys or a newly installed Flatpak. Full cloud
  namespace/SDK restrictions, when encountered, are reported rather than bypassed.
- **Shutdown is cooperative.** Invalidation discards stale work; an already active
  bounded HTTP/native operation can retain temporary memory until it finishes.
  Forced termination cannot promise native cleanup or erase every OS/HTTP copy.
  Do not publish raw browser/network/D-Bus diagnostics or real cache records.
- **Software and service rights differ.** GPL-3.0-or-later notices and compatible
  dependency grants do not authorize OAuth-registration reuse, Yandex API/content,
  trademarks or public service distribution. The attributed Cassette-compatible
  identity remains an explicitly permitted local-prototype adaptation, with
  public service/registration/Linux embedded-auth conditions unresolved rather
  than declared approved or blanket-prohibited. Technical local success and
  source licensing remain independent of public-release authorization.
