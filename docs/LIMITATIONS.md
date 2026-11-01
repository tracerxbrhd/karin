<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Known limitations and acceptance boundaries

## Accepted baseline and new work

The owner accepted the merged v0.2 `master` on Fedora 44 / KDE Plasma / Wayland /
PipeWire, including Flatpak: **17 PASS, 0 FAIL, 1 N/D**. Sign-in, library/search,
actual Music playback, contextual queues, transport/seek/volume, MPRIS/KDE Media
Widget and shutdown form the regression baseline. Hardware media keys are N/D.
PR #13 is merged. Earlier search/UI/audio/packaging failures remain history for
those exact revisions, not the current baseline.

The owner accepted v0.3 runtime
`02325489d23ead9da4471de9cc35e2c063ac8bab` on **2026-10-31**, including
persistent sign-in/restart restoration, real covers in lists/Player Bar/right
pane, real playback, pane mechanics and KDE Media Widget cover/metadata. The
owner also reported successful final Flatpak authorization, playback and
system-integration checks, with no blocking functional defect reported.
Automated checks, that desktop acceptance and public-release rights remain
separate statuses; the documentation-only finalization HEAD is not a new runtime
acceptance revision. Exact provenance belongs in [VALIDATION.md](VALIDATION.md).
Use the
[current Fedora procedure](BUILDING.md#v04-native-music-acceptance-on-fedora).

## Current v0.4 redesign boundary

The owner rejected the Fedora UI/UX of `a9eed4514802df84d73b0608dd1a62930fc1af76`:
partial panel animation, weak content hierarchy, poor wide-space use, a separate
queue route and a primitive footer. Its earlier successful cloud checks remain
historical evidence, not acceptance of those pixels. The current redesign in
Draft PR #15 requires fresh matched GTK captures, whole-surface motion frames,
functional local playback modes and a new owner-local acceptance using the
[v0.4 guide](BUILDING.md#v04-native-music-acceptance-on-fedora).

Track Info and Queue share one logical panel; Queue is local to Karin's loaded
PlaybackContext, not a synchronized Yandex cloud queue. Shuffle and Repeat act
on those admitted available occurrences only, preserve duplicate positions and
never fetch the rest of a remote collection. Modes are app-local policy, retained across queue clear/new context but reset
on explicit logout/account change and close; they are not stored on disk, a new
Music API permission or a persistent offline capability. Repeat One renews
a source only when natural EOS requires another actual playback; a failed
resolution remains failed instead of automatically cycling requests. Stop stays
available through MPRIS/controller even though the visible footer omits it.

Fresh cloud evidence and live Fedora/KDE/Wayland/PipeWire/Flatpak results must
remain separate. Full Flatpak/namespace availability and exact executed tests
are recorded in [VALIDATION.md](VALIDATION.md), not assumed from source review.

- **Motion sampling depends on rendered frames.** A strict Cairo 2× attempt
  missed the early quarter because of software-rendering frame gaps. Its result
  remains failed sampling, not a motion PASS. Matched current motion checks use
  actual NGL/GskGLRenderer on Mesa under Xvfb, with five distinct frame times
  per sequence; this is not physical-GPU or Wayland evidence and does not force
  a production backend. Reduced motion is a different check.
- **Long information remains scrollable.** Long multi-line track/artist/album
  metadata can put verified actions below the initial pane fold. Ordinary wheel
  and keyboard scrolling must retain access; Fedora review should test actual
  fonts/long titles instead of interpreting below-fold content as clipping.
  This is a nonblocking reachability check, not permission to truncate errors
  or conceal controls.
- **Minimum-width footer metadata is compact.** At 420 logical pixels a long
  track title can show only a few characters before native ellipsis; the artist
  remains readable and the complete title remains available through its tooltip,
  accessible label and Track Info. The final cloud review treats this as a
  nonblocking small-width tradeoff, not evidence that every font/long title is
  accepted. The owner should judge the balance on Fedora at the minimum width.
- **Current cloud Flatpak/runtime gaps are explicit.** Flatpak 1.16.6 and Builder
  1.4.4 are installed, but the runtime list is empty and GNOME SDK 50 is absent.
  Host offline release does not establish a full current Flatpak build. Nested
  user namespaces also block the two navigating WebKit fixtures locally; hosted
  CI must exercise them with sandboxing enabled.

## Current v0.3 functional stabilization

At native revision `23174b045cc228e4108e847e801f9264d7a088f9`, saved sign-in
and pane mechanics passed, while every cover surface failed. That historical
FAIL remains recorded. The repair is owner-verified at `0232548` for real covers
in lists, Player Bar and the right pane, plus KDE Media Widget cover/metadata.
Pane mechanics are accepted; its visual design remains unsatisfactory to the
owner. Substantial UI/UX redesign belongs to the next separate iteration.

The URL-generation cause was reproduced independently: internal raster
buckets 128/256/512 were sent as CDN sizes; two published public resources
returned 404 for those sizes and 200 decoded JPEG for presets 200/400/1000.
The correction keeps the internal buckets/limits and existing exact HTTPS
origin. Published protocol-relative and narrowly numeric playlist-revision
variants do not justify arbitrary sources, queries, MIME types or redirects.
Core stable/MSRV checks, counted image-cache tests and extended native GTK
fixtures pass at 1×/2×. Hosted conclusions are tracked on PR #14. Public-sample
HTTP and synthetic fixtures remain separate from the owner's final render-path
acceptance; neither proves every unreported album/playlist surface. The reusable
focused procedure is
[the cover/cache stabilization retest](BUILDING.md#focused-cover-and-cache-stabilization-retest).

The observed metadata cache of about **66 KB after six tracks is not a defect by
itself**. The 256 MiB default is a size ceiling, not a preallocation or expected
music-file size. Disk reuse is established by existing automatic counted-image
fixtures, not inferred HTTP-request counts from user-observed loading speed.
Initial stream buffering remains expected; persistent audio/offline and the full
Wave lifecycle retain their independent documented blockers.

Artwork diagnostics are opt-in and finite, silent by default, and report fixed
stage categories rather than image addresses or personal fields. They do not
identify a particular private image or expose a full request trace. Successful
`must-revalidate` responses may be reused only while explicitly fresh; an expired
raster marked no-stale cannot supply network-error fallback. Other documented
HTTP cache restrictions remain in force.
The malformed-TLS loopback test is a negative handshake/no-downgrade check,
not a claim about a real CDN certificate chain. This cloud execution lacked
Flatpak tooling/an installed GNOME 50 SDK and did not test the package here.
The final owner's Flatpak success is separate evidence; it does not provide a
granular Secret Portal/backend deletion/failure matrix.

## Session and system storage

- **Protected storage is an actual capability, not a service-name check.** Native
  Secret Service and Flatpak Secret Portal/backend availability differ by desktop
  and configuration. Locked, denied, cancelled, corrupt, missing or timed-out
  storage permits ordinary process-only sign-in without plaintext fallback.
  Active cleanup I/O settles first; after a failed result, memory-only login can
  continue while restoration/new persistent writes/cache remain barred.
  The owner's persistent-login/restart and final Flatpak PASS do not identify
  or accept every wallet operation/backend. Synthetic protocol fixtures do not
  establish real verified deletion or failure recovery on each KWallet/GNOME/
  Secret Portal installation; no granular owner matrix was supplied.
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

- **Home refresh can retain an existing preview.** Final source review found
  that Home's refresh fetches new liked/playlist page zero but only assigns it
  when the corresponding model is empty (`load_initial_library_policy`). An
  already populated preview may therefore remain unchanged. Library/detail
  Refresh follows its separate path. This is a nonblocking metadata-presentation
  follow-up, not a replacement for the live account or audio checks.
- **Refusal-driven cache removal can fail.** `CachedCatalog::read` returns a
  non-transport API refusal, but removal of the previous record is best effort.
  If cache I/O returns Busy/unavailable, a later stable read may still find the
  earlier fresh metadata. This source-reviewed failure edge is not covered by
  the successful-removal fixture. It does not authorize a session or supply a
  playback resource; track it as a nonblocking cache-consistency follow-up.

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
  replayed. Broader playlist editing, TrackList, variable speed,
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
  The owner confirmed actual cover/metadata in KDE Media Widget at `0232548`
  and reported successful final Flatpak system integration. Physical media keys
  remain N/D without a separate reported test.

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
