<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Building and running Karin

Karin is a single Rust package. Its minimum Rust series is 1.98
(`rust-version = "1.98"`); compatibility checks pin Rust 1.98.1, including the
Flatpak release profile. Current stable Rust remains the development default.
Native baselines are GTK >= 4.14, Libadwaita >= 1.5 and GStreamer >= 1.20.
`Cargo.lock` fixes Rust dependency versions. The primary manual acceptance target
is Fedora 44, KDE Plasma, Wayland and PipeWire; cloud checks use Debian 13.

## Fedora 44

Install native prerequisites:

```sh
sudo dnf install gcc gcc-c++ make pkgconf-pkg-config \
  gtk4-devel libadwaita-devel webkitgtk6.0-devel gstreamer1-devel gstreamer1-plugins-base-devel \
  gstreamer1 gstreamer1-plugins-base gstreamer1-plugins-good \
  openssl-devel dbus-devel dbus-tools libsecret-devel \
  blueprint-compiler flatpak flatpak-builder python3 \
  xorg-x11-server-Xvfb xorg-x11-xauth
```

The remote player needs the normal `souphttpsrc` HTTPS source and an MP3 decoder
such as `mpg123audiodec` from the GStreamer packages. `gst-inspect-1.0` can check
those components without an account or resource address. Automated remote-media
fixtures additionally use `lamemp3enc` and `xingmux` to generate original synthetic MP3; normal
service playback does not encode or save a music file.

Install Rust through the official [rustup instructions](https://rustup.rs/),
then select the required components. Do not disable TLS or checksum verification.

```sh
rustup toolchain install stable --profile minimal \
  --component rustfmt --component clippy --component rust-analyzer --component rust-src
rustup default stable
pkg-config --modversion gtk4 libadwaita-1 gstreamer-1.0 dbus-1
cargo build --workspace --locked
./target/debug/karin
```

No Yandex account, token, OAuth client secret or subscription is required to build
and run Karin, its automated tests or its local player. The ordinary connection
probe is anonymous. Gate B1's separate opt-in OAuth diagnostic is intended only
for a consented local desktop experiment; it is not application sign-in or
Yandex Music stream playback. The default sign-in provider is the Cassette-compatible
session prototype below; it requires no end-user OAuth configuration.

<a id="v04-native-music-acceptance-on-fedora"></a>

## v0.4 — one complete music-experience acceptance on Fedora

Use the current source-matched native archive linked in
[Draft PR #15](https://github.com/tracerxbrhd/karin/pull/15). Confirm the source
revision in its build information and verify `SHA256SUMS` before running ordinary
`./karin`. A source build uses the prerequisites above and
`cargo +1.98.1 build --release --locked`, then `./target/release/karin`.
The before revision `a9eed4514802df84d73b0608dd1a62930fc1af76` passed cloud
checks but **failed owner UI/UX acceptance**. Earlier v0.3 audible playback and
Flatpak acceptance do not pass this new revision. Keep PR #15 Draft.

1. Restore the saved session or use ordinary **Войти**. Confirm the intended
   Music account, real library/covers and an actual chosen track audible through
   PipeWire. No developer credentials, pasted token, diagnostic flags, secret
   transfer or browser/network recording belongs in this run.
2. Check Home, Search and Library as the only primary music destinations.
   Home should show real liked tracks/available playlists with useful covers
   and concise hierarchy, not giant duplicate shortcut cards. Test library
   selection, playlist/album details, four search categories, Show All/Back,
   pagination, Retry and server-confirmed playlist creation. Inspect sparse and
   long-title collections without publishing their contents.
3. Open Track Info with the current cover/Ctrl+I. Open Queue with the footer
   button/Ctrl+J: content must change in the **same** right surface without a
   close/reopen. Switch back using the cover or Details; Details must not play.
   Repeated cover activation keeps information open; repeated Queue activation
   may explicitly close it. Verify X/compact Back and focus return.
4. Watch opening and closing with normal system animation enabled. Background,
   border, header, cover, text and buttons must slide together throughout the
   motion, with no instant text, flashing controls, central-width jump or hidden
   row actions. Check approximately quarter/half/three-quarter progress, not
   just the final open state. TrackInfo ↔ Queue keeps stable width/header/close.
   Check the **first** panel appearance after a fresh ordinary application start,
   including restored presentation, then repeat Track Info/Queue open/close
   after the view is warm. Report first-open versus warm-repeat behavior
   separately: later smooth motion does not pass a stalled first open. Keep
   Fedora's ordinary renderer and settings; no backend or cache override belongs
   in this acceptance run.
   The automated motion gallery measures a later cycle after explicit test-only
   graphics initialization. It does not accept cold responsiveness: delayed
   first-paint phases occurred on both old and modern NGL/Vulkan test stacks.
   Report any first-open stall even if repeated openings are smooth.
   The PR's current closing sequence is ordered 100→75→50→25→0 by openness;
   old observer frames use elapsed-quarter labels. These cloud samples do not
   substitute for this run with Fedora's normal renderer and settings.
   Then check the desktop reduced-motion setting separately.
5. While each panel mode is open, click/scroll the uncovered content, navigate,
   change the playing occurrence and resize compact→medium→wide→compact.
   Preserve the chosen open mode and permanent Player Bar. Open Diagnostics and
   return: restore intent. Inspect 420×720, 720×650, 900×650, 1280×800,
   1440×900, **1650×930**, 1920×1080 and actual maximize; light/dark and 1×/2×.
   Check useful width, density, native window controls, long text, missing art,
   focus and access to all actions. The narrow panel must not make Library
   unusable. Compact uses explicit Back rather than a dismissing backdrop.
   At 720 pixels also expand navigation labels: if remaining music content is
   below 620 logical pixels, the dedicated panel subview is intentional. Long
   information may scroll; verify wheel/keyboard access to below-fold actions.
   At 420 pixels judge the footer title's native ellipsis and full tooltip/info
   access separately from the readable artist and working compact controls.
6. Play rows using mouse and Enter/Space. Verify exact context occurrences,
   duplicate tracks, manual additions, current/noncurrent removal, clear and
   unavailable positions. Queue's current/next order must match what actually
   plays; a loaded page remains labelled partial. With information open, change
   tracks using Next/Previous, queue selection and EOS: metadata/art update
   together in place. Pause/Seek/Volume preserve an explicit Details selection;
   the next actual current occurrence resumes following without restarting the
   panel. Pane-only operations must never restart sound.
7. Exercise Shuffle without changing the current song; follow several Next
   transitions and use Previous to revisit actual history. Disable Shuffle and
   confirm the original context order resumes from the current occurrence.
   Cycle Repeat **Off → Queue → One → Off**. Test natural EOS for the same
   occurrence under One, whose active control must visibly show **1**, and
   manual Next/Previous under One, Queue wrapping,
   Shuffle+Queue and Shuffle+One. Confirm unavailable tracks are skipped and a
   failed source has a visible error rather than an automatic request loop.
   No visible Stop button should appear; Stop remains a supported system action.
8. Check KDE Media Widget cover/title/artist/duration/progress, transport,
   seek/volume and modes against Karin's buttons. If `playerctl` is installed,
   use its existing session-bus controls, including external mode writes:

   ```sh
   playerctl --player=io.github.tracerxbrhd.Karin shuffle On
   playerctl --player=io.github.tracerxbrhd.Karin loop Track
   playerctl --player=io.github.tracerxbrhd.Karin next
   playerctl --player=io.github.tracerxbrhd.Karin loop Playlist
   playerctl --player=io.github.tracerxbrhd.Karin shuffle Off
   playerctl --player=io.github.tracerxbrhd.Karin loop None
   playerctl --player=io.github.tracerxbrhd.Karin stop
   ```

   Confirm native mode state follows external writes and vice versa, capabilities
   match real next/previous choices, and Stop actually stops. Test physical media
   keys separately; do not infer them from `playerctl` success.
9. Recheck cache-off/Clear, Preferences, safe logout confirmation Cancel,
   remembered-login restart and normal close. Presentation/cache operations
   preserve live session/audio unless explicitly changed. Confirmed Logout stops
   playback and clears personal panel/queue/cache and saved credentials, with
   unknown cleanup outcomes kept visible. Repeat relevant auth/session/music/
   modes/panel/KDE checks in the **current Flatpak** when available, without
   broadening bus/filesystem permissions or disabling sandboxing.

Report exact revision, native/Flatpak, desktop environment and anonymized
PASS/FAIL/N/D per scenario. The cloud matrix contains 96 current cases and
honest before counterparts, plus page/state and animation-frame evidence; see
[UI_QUALITY_GATE.md](UI_QUALITY_GATE.md#v04-native-music-gate). It is not Fedora
acceptance. Never publish personal screenshots, account/library data, audio,
cookies, tokens or signed addresses. Public service-distribution conditions,
offline audio, My Wave and lyrics retain their separate gates. No merge is
authorized by this procedure.

<a id="v02-catalog-and-delivery-info-acceptance-on-fedora"></a>
<a id="v03-seamless-experience-acceptance-on-fedora"></a>

## v0.3 — one Fedora acceptance session

This retained procedure describes the accepted v0.3 runtime, including its old
1180-pixel collapsed-pane layout. For the current v0.4 nonmodal information
presentation and owner review, use [the v0.4 guide above](#v04-native-music-acceptance-on-fedora)
and ADR-0021; the old dismissing overlay is not the current requirement.

The owner accepted v0.3 runtime
**`02325489d23ead9da4471de9cc35e2c063ac8bab`** on **2026-10-31** on Fedora
44 / KDE Plasma / Wayland / PipeWire: persistent sign-in/restart restoration,
real covers in lists/Player Bar/right pane, actual playback, pane mechanics and
KDE Media Widget cover/metadata. Final Flatpak checks were also reported
successful for tested authorization, playback and system-integration scenarios.
See [the exact acceptance scope](VALIDATION.md#final-owner-v03-acceptance-at-0232548-2026-10-31).
No blocking functional defect was reported. The pane's visual design remains
unaccepted; substantial UI/UX work moves to a separate iteration.

The procedure below remains the reproducible native/Flatpak regression guide.
Documentation-only finalization in [PR #14](https://github.com/tracerxbrhd/karin/pull/14)
does not rename the accepted runtime SHA; final-head CI is tracked on that PR.
This is functional-prototype acceptance, not a public-release approval.

Check the build's source revision and `SHA256SUMS` before running. A successful
MSRV CI run supplies `karin-fedora-prototype-1.98.1`, with an archive, build
metadata and licenses. It is a native Ubuntu 24.04 x86_64 test binary, not a
Flatpak or production release. If that binary is unavailable/incompatible,
build the same PR revision locally using the prerequisites above:

```sh
rustup toolchain install 1.98.1 --profile minimal
cargo +1.98.1 build --release --locked
ulimit -c 0
./target/release/karin
```

For the extracted archive, run ordinary `./karin`. Use your own local desktop,
with network/browser/GStreamer debugging disabled and no terminal recording.
No Client ID/secret, pasted token, application registration, manual URL transfer,
experimental flag, VPN change or sandbox override belongs in this journey.
Do not use automation's `--ui-fixture` or `--smoke-test` for real acceptance.

### Focused cover and cache stabilization retest

The native `23174b0` test had a real-cover FAIL. Its repair is owner-accepted at
`0232548` for lists, Player Bar and right-pane images, with actual KDE Media
Widget cover/metadata also confirmed. Keep the earlier FAIL as revision history;
additional surfaces and backend scenarios below are reusable regression checks,
not invented individual results from the final owner report. Pane redesign is
deferred. Match the artifact's exact source revision and checksums when repeating
the procedure.

1. Start ordinary `./karin` on the local Fedora desktop. If a saved session is
   available, confirm it still restores the intended Music account after fresh
   validation. No technical OAuth inputs or user credentials go to Cloud/CI.
2. Open tracks and collections whose covers you recognize. Check track lists,
   search/details, albums, playlist tiles, Player Bar and the right pane while
   selecting/playing the same item. Test missing/unavailable artwork separately:
   a fallback is correct there, not a successful real-cover result. Repeat in
   light/dark and HiDPI; scroll and resize without losing loaded images or sound.
3. In Storage, keep data caching enabled with its default **256 MiB ceiling**.
   Observe measured usage, reopen an already loaded album/track and restart the
   app. Record cover reuse separately from initial image display. Fast appearance
   alone does not prove a disk hit: automatic integration tests must count HTTP
   requests/cache reads, and any local diagnostic must use only the approved
   fixed categories. About 66 KB of metadata for six tracks is not an audio-cache
   failure or an expected music-file size.
4. Disable data caching and open another item. Persistent image reads/new writes
   must stop; already saved files may remain until Clear. Re-enable it, then
   clear while a real track plays. Confirm the clear result while sign-in,
   playback/context, visible texture and KDE/MPRIS presentation remain intact.
5. Briefly repeat search, collection navigation, click/Enter/Space playback,
   Next/Previous, Pause/Resume/Stop, Seek and Volume. Opening/closing information
   must not restart audio. Do not expose Wave, similar tracks or offline controls
   to substitute for this functional regression check.
6. Turn remembering off while playing: saved-credential deletion must be
   confirmed while the current session/sound may continue. Close/reopen and
   check that sign-in is required; sign in normally if continuing. Then test
   Cancel/default confirmation before explicit Logout. Confirmed Logout stops
   music, clears personal cache and deletes the saved credential. Unknown cleanup
   remains incomplete with Retry. Do not damage a personal keyring to invent a
   failure; automated locked/missing/corrupt/cancel cases use synthetic backends.
7. Report exact revision, native/Flatpak and anonymized PASS/FAIL/N/D for real
   covers by surface, persistence/reuse, cache-off/clear, restored login/forget,
   sound/queue/MPRIS and close. Do not share titles, account IDs, full URLs,
   response bodies, private cache files, secrets or personal screenshots.

### Safe opt-in artwork diagnostics

Ordinary runs keep image-stage diagnostics disabled. For one local reproduction,
close any existing Karin window, use your own already authorized session and run
this option **alone** from the extracted acceptance directory:

```sh
ulimit -c 0
./karin --artwork-diagnostic
```

This opens the ordinary application; it is not a token-entry tool or a separate
image downloader. Do not combine it with `--ui-fixture`, `--smoke-test`, delivery
diagnostics or other flags. Leave HTTP/WebKit/GStreamer tracing and terminal
recording disabled. No TLS, origin or redirect override is needed.

The summary emits each fixed category at most once per process, at most **45
lines**, with prefix `Karin artwork:`. Categories distinguish:

- `metadata-missing`, `uri-rejected`, `request-not-started`;
- `visibility-deferred`, `request-queued`, `queue-busy`, `cancelled`;
- `dns-failure`, `tls-failure`, `network-timeout`, `connection-failure`,
  `network-failure`, and `http-response` with only a numeric HTTP status;
- `http-refused`, `content-type-unsupported`, `decoder-unavailable`,
  `decode-failed`, `image-too-large`;
- `memory-cache-hit`, `disk-cache-hit`, `cache-miss`, `cache-bypassed`,
  `cache-read-failed`, `cache-write-failed`;
- `pixels-ready`, `gtk-displayed`, `gtk-lifecycle-unavailable`.

Examples of the allowed result shape are:

```text
Karin artwork: event=http-response http=200
Karin artwork: event=pixels-ready
Karin artwork: event=gtk-displayed
```

Only share those category lines and the exact source revision if a report is
needed; no complete log, image URL/path/query, response body, title, account ID,
cookie, token or private cache record is required. Categories are process-wide
stage presence, not per-image correlation or network-request counts. A disk-hit
category can support the local observation, while counted loopback integration
tests establish exact reuse. A queued/deferred image is not a decode success,
and `pixels-ready` is not yet `gtk-displayed`. An expected missing cover still
uses its native fallback.

Actual real-cover display is owner-confirmed for the stated surfaces at
`0232548`; the final native and Flatpak report remains separate from these
diagnostic categories. A later runtime change needs its own targeted regression.
The report does not identify every Secret Portal/backend or prove individual
failure/deletion cases. Keep the current sandbox permissions. Persistent audio
remains absent because storage rights are unconfirmed; see
[AUDIO_CACHE_REVIEW.md](AUDIO_CACHE_REVIEW.md). My Wave remains hidden pending
its full service contract, and public OAuth-distribution rights remain separate.

### Session and preferences

1. Open Settings and inspect authorization/storage availability. Native builds
   use the available Secret Service keyring; Flatpak uses the available Secret
   Portal/backend. Running KWallet/GNOME or a portal name alone is insufficient:
   the following save/restore/delete sequence must actually succeed. If protected
   storage is unavailable, ordinary process-only login must remain possible
   without plaintext fallback. Active cleanup I/O must settle first; a failed
   cleanup remains visible with Retry and permits only memory login while old
   restoration/new persistent credentials/cache stay barred in the current
   process; cross-restart protection requires a successfully saved barrier.
2. With **Запоминать вход** enabled and supported, click **Войти**, complete the
   existing Yandex dialog and wait for Music account validation and the real
   library. Any native keyring unlock prompt belongs to the desktop; cancel it
   if desired and verify an honest storage failure, without exposing a password
   field in Karin. Play an entitled real track through PipeWire. A Yandex page,
   saved token or generated tone is not successful Music playback.
3. Close Karin normally and reopen the same build. Check a visible restoring
   state, then the correct account/library after Music validation, without
   repeating ordinary login. Verify real sound and the original queue/transport
   behaviors again. A transient network failure must be distinguishable from an
   invalid token or unavailable/locked store; Retry must not silently change
   identity or report an unvalidated account as authenticated.
4. Change system/light/dark appearance, cache settings and track-pane preference,
   close/reopen and check persistence. Settings-save failures must be visible;
   an older asynchronous save must not undo a newer choice. New loading/error
   state itself must not become a persisted preference.
5. While a real track plays, turn remember-login off. Saved-credential deletion
   must be confirmed while the current in-memory session may continue. Close
   and reopen: ordinary login is required again. Re-enable remembering and log
   in normally if continuing the restoration tests. If a previously available
   backend is now unavailable, its saved enabled preference must remain honest
   and may still be turned off to record forget intent; deletion stays pending
   with Retry. Enabling again requires supported protected storage.
6. Open the account avatar and choose **Выйти из аккаунта**. Cancel/Enter/Escape
   at the default **Отмена** response must preserve account, queue and sound.
   Confirm explicitly: stop audio, clear personal presentation and persistent
   metadata/covers, and delete the saved credential. A failed/unknown delete or
   clear must stay visible with Retry; it must not claim final success. Reopen
   after confirmed logout and verify no automatic old login. Test cancellation,
   late loading and a fresh login without reviving old account data.

When successfully saved, the durable pending-forget barrier prevents restoration
while cleanup remains unresolved. If recording the barrier itself fails, Karin
must say restart protection/final forget are unconfirmed; retry cleanup before
relying on a later restart to remain signed out. Portal forgetting removes Karin's authenticated encrypted token
file, not the portal's per-app master key or Yandex's server grant. Native
forgetting verifies matching saved items are absent. No refresh-token or server
revocation mechanism is claimed. Do not inspect or share keyring secret values.

Locked/unavailable/corrupt storage, invalid-token and concurrent-operation cases
are covered by synthetic fixtures; an actual desktop result is recorded only
when observed. Do not damage a personal keyring to manufacture a failure. If an
ordinary wallet prompt/error occurs, report its fixed category, not a screenshot
or complete D-Bus trace.

### Cache and normal music use

1. In **Настройки → Хранилище**, verify caching defaults to enabled, with a
   256 MiB limit and a supported 16 MiB–4 GiB range. Load real library, playlist,
   album and track details/covers, then check measured usage. Repeat a read and
   exercise explicit Refresh. Fixture tests count network requests; apparent
   speed alone does not prove a cache hit or freshness.
2. Check saved/stale fallback presentation during an ordinary network failure:
   previously loaded metadata must be labelled as saved/potentially outdated.
   Cached records must not confirm current authorization, favorite membership,
   playlist revisions or writes. Do not interpret saved metadata/covers as
   downloaded or offline-playable music.
3. Disable caching. Existing usage may remain until explicit clear; persistent
   content reads and new writes must stop. Re-enable it and verify normal
   bounded reuse. Change the limit and confirm eviction/usage behavior. A cache
   error must not invalidate the account or silently replace live data.
4. While music plays, choose **Очистить кеш**. Observe clearing and its actual
   result/errors. Current playback, context, metadata/texture presentation,
   remembered session and live MPRIS cover must remain intact. Old in-flight
   loads must not repopulate the cleared persistent store. Logout is different:
   it deliberately stops audio and purges personal cache along with forgetting
   the saved credential.
5. If you have another account of your own and choose to test it, use ordinary
   confirmed Logout followed by ordinary Login. No old account's cached library
   may appear as the new account's data. Automated fixtures cover separate
   synthetic accounts; never import a browser profile or another user's token.

Corruption, interrupted writes, cache-hit/miss/stale, account isolation and
clear/disable races use isolated synthetic files in automated tests. Do not
upload a real cache record or edit personal credentials to reproduce them.
Persistent audio/offline support has a separate rights/contract gate in
[AUDIO_CACHE_REVIEW.md](AUDIO_CACHE_REVIEW.md). There is no inert Offline switch
or hidden music download. The existing resolver/player still streams eligible
resources through its guarded path.

### Track information, playlists and regression

1. Above 1180 logical pixels check the preferred right track-information pane;
   at 1180 or narrower it is a collapsed overlay and must not auto-open on Play.
   Click the current cover or press Ctrl+I to open it explicitly; close/reopen
   and verify the persisted wide-pane preference. Verify actual large cover/title/artist/album,
   selected-versus-playing status, close/focus return and verified album/artist
   actions. Opening/closing or resizing information must not restart/stop audio.
   Missing identities must not produce guessed navigation or fake similar tracks.
2. Inspect playlist cover tiles with one, many, long-title and missing-cover
   records. Use mouse and Enter/Space to open details. Check loading, empty,
   failure/Retry and narrow-to-wide wrapping. If creation is exposed, deliberately
   create one test playlist (private by default) and require server-confirmed
   success. Public visibility must be an explicit choice. An ambiguous
   result must remain unknown; Refresh reads the server and must not replay POST
   or add a fake local tile. A known service refusal removes Create for this
   account while normal library/playback remain usable. Do not repeatedly create
   after an unknown outcome.
3. From liked tracks, an album, playlist and unified search, activate a playable
   row using click, Enter and Space. The exact occurrence must become current;
   source order/duplicates, Next/Previous/EOS, manual additions and the visible
   partial-collection boundary must remain correct within 500 positions. Details
   are a separate action. Navigation must not replace an active context.
4. Search Latin/Cyrillic queries across tracks/albums/artists/playlists; check
   Show All, paging, Back, Retry and independent section failures. Keep the
   accepted one-batch track normalization and unavailable-item handling intact.
5. Exercise the accepted player bar's Play/Pause/Resume/Stop, progress/Seek,
   Volume, queue and clean close while hearing a real Music track. Check KDE's
   Media Widget and media keys when available; compare changes in both
   directions with Karin. MPRIS registration alone does not prove those controls.
6. Resize around 420×720, 900×650 and 1440×900, maximize/restore, both themes and
   HiDPI. Check useful density, headers/window controls, full scroll reachability,
   long text, keyboard focus, track pane, grid and PreferencesDialog. Preserve
   the expandable icon rail, avatar, safe logout and accepted player composition.

Optional command-line MPRIS checks, in the same user's desktop session:

```sh
playerctl --player=io.github.tracerxbrhd.Karin status
playerctl --player=io.github.tracerxbrhd.Karin pause
playerctl --player=io.github.tracerxbrhd.Karin play
playerctl --player=io.github.tracerxbrhd.Karin position 5+
playerctl --player=io.github.tracerxbrhd.Karin volume 0.5
playerctl --player=io.github.tracerxbrhd.Karin next
playerctl --player=io.github.tracerxbrhd.Karin previous
playerctl --player=io.github.tracerxbrhd.Karin stop
```

The ordinary shortcuts remain Ctrl+H Home, Ctrl+L Library, Ctrl+F Search,
Ctrl+J queue panel, Ctrl+B navigation and Ctrl+, preferences; Ctrl+I opens track
information and Ctrl+Q closes Karin.
My Wave and similar-track sections stay hidden when their complete real
contracts/capabilities are unconfirmed. Unsupported source stubs are not features.

### Repeat inside the current Flatpak

Build/install the same revision using the GNOME 50 / Freedesktop 25.08 procedure
below, without filesystem/session-bus/sandbox overrides. Repeat the same session,
settings, cache, workspace, actual sound and MPRIS checks in that installed app.
Native keyring success does not establish a working sandbox portal backend;
missing Secret Portal must produce the explicit safe session-only fallback.

XDG config/data/cache live in the app's private Flatpak space. Cache clearing
must not remove the protected credential envelope or preferences. Preserve the
exact MPRIS own-name permission; no broad session-bus, host-filesystem or direct
Secret Service grant is assumed. A full package/build/install result is passed
only when actually run, independently of vendored native/offline compilation.

### Report only safe results

Share the exact revision, native/Flatpak, backend kind/availability and anonymized
PASS/FAIL/N/D for restore/forget, settings, cache/clear/isolation, pane/tiles,
search/contextual sound, KDE controls and shutdown. Record unsupported services
as unavailable or not established, not a synthetic success. New owner acceptance
belongs in [VALIDATION.md](VALIDATION.md).

Do not publish credentials, portal keys, secret-item values, parameterized URLs,
headers, raw JSON, account/track/playlist names or IDs, cache records, media,
`playerctl metadata`, complete D-Bus/network traces or personal screenshots.
Use only the fixed category/stage/numeric HTTP supplied by native error details
when a specific failure needs diagnosis. Public distribution/registration rights
and audio offline rights remain separate from technical local acceptance.

## Music sign-in and Fedora acceptance

<a id="experimental-music-session-and-fedora-acceptance"></a>

Current v0.4 acceptance uses the combined guide above. The following provider
contract and historical focused retests explain earlier findings; they are not
instructions to select old PR artifacts, prerequisites for ordinary login or a
replacement for current persistence/real-playback checks.

The default build now enables `cassette-auth` and WebKitGTK6. The normal
`Войти` action attempts real Yandex login using the published registration and
session grant used by Cassette. This is **not** Karin's own-registration PKCE
route, and it is not the previously blocked provider. No Client ID, Client secret,
app registration, token paste, device code or URL copy is required.

Use a trusted local Fedora desktop/terminal, not Cloud/CI, a terminal recorder,
remote support session or shared console. After installing the prerequisites
above, prepare one release build and run it:

```sh
rustup toolchain install 1.98.1 --profile minimal --component rustfmt --component clippy
cargo +1.98.1 build --release --locked
ulimit -c 0
./target/release/karin
```

For convenience, a successful matching MSRV CI run also uploads
`karin-fedora-prototype-1.98.1`, containing a native tar archive, `SHA256SUMS`,
build/source metadata, runtime instructions and licenses. Verify it with
`sha256sum -c SHA256SUMS` before extracting/running. This Ubuntu 24.04 x86_64
binary is a test artifact, not a Flatpak or production release; Fedora ABI/startup
is unverified. If unavailable or incompatible, use the canonical local source
build above. Select the matching successful PR revision, not an older artifact.

### One Fedora navigation retest after `77a1a27`

The owner confirmed that `77a1a27` opens the real Yandex page on Fedora, then
Karin reports `UntrustedNavigation` after roughly five phone-input digits or
when using QR sign-in. The rejected browser event has not been captured. This
is not evidence of Music API 403, a wrong password, VPN trouble or missing rights.

Use the new matching successful `karin-fedora-prototype-1.98.1` artifact from
[PR #8](https://github.com/tracerxbrhd/karin/pull/8), verify its `SHA256SUMS`, and
enter the extracted directory. Run once from a private local terminal with this
output filter, which creates no full log file:

```sh
ulimit -c 0
./karin 2>&1 | sed -n '/^Karin auth navigation: /p'
```

Click `Войти`, use your own account and reproduce the phone-entry interaction.
If it still fails, copy **only the first line beginning `Karin auth navigation: `**
from that attempt, the build revision and the coarse native UI outcome. For the
QR path, cancel/close the current attempt, reopen `Войти` and repeat the ordinary
Yandex QR interaction; report its first line separately if one appears. Do not
record the terminal, use `tee`, save unfiltered stderr or enable full browser,
JavaScript or network logging. Do not send entered digits, QR contents, browser
URLs, screenshots of login/account data, cookies, tokens or state.

The line records only fixed event/rejection/frame categories and, for a safe
known public origin, normalized scheme/host/port and fragment presence. Malformed,
userinfo-bearing, OAuth-result and unknown-host addresses become fixed redacted
categories; their address text is not retained. `frame=Unknown` is deliberately
not described as main-frame or iframe evidence. A blocked auxiliary transition
does not by itself mean the sign-in attempt failed. Absence of a line means no
rejection was recorded, not that authorization or Music access succeeded.

If the page now continues, complete the same attempt and continue with Music
account/library/search acceptance below. Do not switch OAuth identity, region,
mechanism or network settings to hide a refusal. The actual Yandex phone and QR
flows still require this local check; the interactive loopback fixture is not
their live acceptance.

Run **without** `--ui-fixture`, `--smoke-test` or the separate
`--oauth-diagnostic`; those are not the real sign-in journey. No secrets belong
in command arguments, environment variables, files, chat or GitHub. The consent
page may name the upstream shared OAuth registration used by Cassette rather than
Karin's own registration. Inspect the actual Yandex origin shown in the dialog
and grant access only to your own account if you choose to proceed. Code licensing
does not establish service/identity distribution rights; see
[the scoped prototype decision](adr/0011-cassette-session-auth-prototype.md).

Perform this one substantive acceptance run on Fedora 44/KDE/Wayland/PipeWire:

1. Click `Войти`. Confirm the native dialog loads a real Yandex page and shows only
   the trusted origin, without exposing full URLs or developer fields. Sign in to
   your own normal account inside that page. Karin never reads its password form.
2. After consent, the dialog must move automatically through return/state,
   own-session cookie collection and OAuth exchange to **Music account validation**.
   Do not copy any code, token or URL. A Yandex login or issued token alone is not
   success. The dialog must close as authenticated only after Music validation.
3. Verify your actual liked tracks and personal playlists load; choose a playlist
   and verify its contents. Search for a familiar track, album and artist; inspect
   the corresponding returned results and details. Empty libraries are legitimate
   only if they match your account. Add tracks to the queue and confirm ordering.
   Activating a playable track row starts it in its returned collection context;
   details use a separate action. Check Next/Previous/EOS and explicit appends.
4. Sign out. Check that account/library/queue state and prior errors clear. Start
   another login, cancel it, retry and close the window; delayed results must not
   sign you back in. For v0.3, restart behavior follows the protected remember-login
   setting and confirmed forget outcome described in the current guide above.
5. Check approximately 420×720, 900×650, 1440×900 and maximized, both themes, HiDPI,
   HeaderBar/window controls, scrolling, keyboard focus/activation and Ctrl+Q.
   Stale floating Music 403 text must not remain; errors belong in native rows.
6. If a stage fails, share only the revision, `cassette-auth` build, failed stage,
   safe fixed category and HTTP status when supplied. Examples are trusted-return/
   state mismatch, missing session cookie, cookie-read failure, `/token` refusal
   or unsupported response, Music account 403, or library/search schema rejection.
   Do not publish cookies, tokens, full URLs, raw JSON/headers, account identifiers,
   private library screens or login screenshots. Do not change identity, region or
   OAuth mechanism during this acceptance run to hide a failed result.

The owner accepted the merged v0.2 normal sign-in/library/search and real
playback path, including KDE Media Widget and the installed Flatpak. Earlier
`8f1b09d`/`f0aa2bb` results below remain historical stages. New v0.3 protected
storage/cache/workspace behavior still requires the current combined acceptance
run above; source review and fixtures cannot supply its real Fedora result.

The selected return is exact `https://music.yandex.ru/` with matching fresh state;
a different origin/path or missing state must report the specific validation
failure rather than accept a substring. Cookie collection uses only this dialog's
ephemeral session and fixed official OAuth token URI, never another browser's
profile. Cookies/sessionid remain memory-only and cancellation clears browser
state. A Music-validated token may be remembered only through the protected
v0.3 backend. Local forgetting is not provider consent revocation; no refresh
flow is introduced. See ADR-0018 and the current guide for restore/delete outcomes.

## WebKit authentication build and local fixture checks

WebKitGTK6 is a default prototype dependency, confined to the sign-in dialog.
`webkit6` 0.5 matches GTK Rust 0.10; 0.6 targets GTK Rust 0.11 and is not selected.
Fedora needs `webkitgtk6.0-devel` (included above); Debian/Ubuntu needs:

```sh
sudo apt-get install libwebkitgtk-6.0-dev
```

`cassette-auth` enables `auth-webkit`; `--all-features` additionally includes the
library-only legacy Device Flow experiment without exposing it in the GUI.
`cargo build --no-default-features` is an intentional compatibility build without
the real provider, not the artifact for Fedora authentication acceptance.
GNOME 50 already supplies WebKitGTK6; the existing Flatpak manifest enables
`cassette-auth` without changing runtime/SDK versions or permissions.

Display-dependent WebKit tests are ignored by the ordinary test command. Run each
in a separate display/D-Bus process with the WebKit sandbox enabled. They use only
synthetic/local pages and credentials, never a Yandex account. The full test needs
Openbox and `wmctrl` with a 1920×1080 Xvfb screen for actual maximization; optional
captures also need `xdotool` and ImageMagick's `import`. Install these test tools
separately from normal application runtime prerequisites.

```sh
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- xvfb-run -a \
  cargo test --lib --locked --all-features \
  webkit_auth_construction_and_shutdown_without_navigation -- --ignored --test-threads=1
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' sh -eu <<'SH'
openbox --sm-disable >/tmp/karin-webkit-openbox.log 2>&1 &
wm_pid=$!
trap 'kill "$wm_pid" 2>/dev/null || true; wait "$wm_pid" 2>/dev/null || true' EXIT
attempt=0
until wmctrl -m >/dev/null 2>&1; do
  attempt=$((attempt + 1))
  test "$attempt" -lt 50
  sleep 0.1
done
cargo test --lib --locked --all-features \
  webkit_auth_local_startup_cookie_isolation_and_cancel -- --ignored --test-threads=1
cargo test --lib --locked --all-features \
  webkit_auth_interactive_navigation_frames_and_first_rejection -- --ignored --test-threads=1
SH
```

The second invocation is a separate GTK/WebKit process. Its loopback page reacts
to actual synthetic keyboard input and a QR-like test button by changing its hash
and creating/redirecting child frames. It checks ordinary fragments, blocked
auxiliary and top-level transitions, iframe callback isolation, strict returned
state, cancellation/retry/late events and diagnostic redaction. It never uses a
real phone, QR payload, Yandex account or Music API. Keep the WebKit sandbox
enabled; if namespace creation is unavailable, report the test as blocked rather
than disabling the sandbox or treating a startup-only test as a passed flow.

`--smoke-test` and the capture helper's explicit `--ui-fixture` mode inject an
offline provider. They exercise dialog/layout/lifecycle without contacting Yandex;
they are not evidence of real sign-in. Never use `--ui-fixture` for the owner
acceptance above. Hosted sandbox checks and remaining namespace restrictions are
recorded in [VALIDATION.md](VALIDATION.md).

## v0.4 strict native visual checks and CI placement

The strict populated-matrix and whole-surface-motion fixtures are separate from
the functional/compiler baseline. CI retains Ubuntu 24.04 stable/MSRV jobs for
29 functional selectors, packages and both sandboxed navigating WebKit fixtures.
Dedicated `Native UI <toolchain> (Debian 13)` jobs use the digest-pinned official
Rust trixie image and GTK >= 4.18 / Libadwaita >= 1.7 for the two strict selectors,
both scales, both compilers and matching captures. The compiled name partition
checks all 31 = 29 + 2 selectors; none is dropped. These modern **test** libraries
do not change Karin's GTK >= 4.14/Libadwaita >= 1.5 application baseline, current
Flatpak SDK or production renderer.

The motion fixture records a fixed graphics-initialization cycle separately:
one real normal opening, six bounded offscreen renders of the actual mapped
panel while open, then real closing. Setup quarters are not assessed; no setup
render becomes a PNG, changes UI progress/clock or supplies measured evidence.
Only the next real normal 240 ms native cycle is strictly measured with original
tolerances and five distinct frame identities. This is not cold-start acceptance
or a production prewarm. Retain the raw initialization alongside the measured
frames, and keep Fedora's fresh-first-open gate separate.

For equivalent local test checks on a modern native stack with the fixture tools
above, use synthetic captures only:

```sh
pkg-config --atleast-version=4.18 gtk4
pkg-config --atleast-version=1.7 libadwaita-1
for toolchain in stable 1.98.1; do
  for scale in 1 2; do
    if [ "$scale" = 1 ]; then screen=1920x1080x24; else screen=3840x2160x24; fi
    for selector in native_design_matched_matrix_and_populated_surfaces \
      native_universal_panel_whole_surface_motion_and_mode_switch; do
      GTK_A11Y=none GSK_RENDERER=ngl GSK_DEBUG=full-redraw \
        GDK_SCALE="$scale" GDK_DPI_SCALE=1 KARIN_V04_ASSERT_DENSITY=1 \
        KARIN_TEST_CAPTURE_DIR="$PWD/build/ui-captures/strict-$toolchain-scale$scale" \
        dbus-run-session -- xvfb-run -a -s "-screen 0 $screen -dpi 96" \
        cargo +"$toolchain" test --lib --all-features --locked "$selector" \
        -- --ignored --test-threads=1
    done
  done
done
```

Retain actual rendered frame identities/progress, source/library/renderer records
and original before/current images. Modern CI artifacts are
`karin-native-visual-<lane>-<github.sha>`; Ubuntu functional images remain
`karin-ui-<github.sha>`. The initial `709f5b3` hosted GTK 4.14/Adwaita 1.5/Mesa
25.2 strict-motion sampling failed, with a matching approximately 915 ms first
reveal-paint gap reproduced locally; its internal cause is not attributed.
Cold modern NGL and actual Vulkan attempts failed phase coverage too; changing
backend/distribution alone is not a proven fix. The later graphics-initialized
measured PASS does not erase those results, accept cold responsiveness, prove
physical GPU/Wayland behavior or replace actual hosted conclusions. See [VALIDATION.md](VALIDATION.md).
Never disable animation, duplicate quarter frames, extend production timing or
relax tolerances to make a renderer pass. Use ordinary `./karin` with Fedora's
normal renderer for the separate owner acceptance; these test flags do not
belong in that run.

## v0.3 isolated persistence and workspace fixtures

The following tests use synthetic stores/snapshots and must run in their own
session buses/displays. They do not use a real wallet, portal, account or cached
library. The secure-storage test requires the explicit test guard as well as
`dbus-run-session`; never select it against a personal desktop bus.
Actual input/capture fixtures additionally need `xdotool`, Openbox, `wmctrl`
and ImageMagick's `import`; the persistence integration fixture uses the
original generated-MP3 GStreamer plugins described above.

```sh
KARIN_TEST_SECRET_BUS=1 dbus-run-session -- \
  cargo test --lib --all-features --locked \
  native_secure_storage_secret_service_and_portal_protocol -- --ignored --test-threads=1

GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' \
  cargo test --lib --all-features --locked \
  native_workspace_track_pane_actions_state_and_cleanup -- --ignored --test-threads=1
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' \
  cargo test --lib --all-features --locked \
  native_workspace_playlist_grid_input_states_and_resize -- --ignored --test-threads=1
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' \
  cargo test --lib --all-features --locked \
  native_preferences_persistent_controls_snapshot_updates_and_cleanup -- --ignored --test-threads=1
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' \
  cargo test --lib --all-features --locked \
  native_playlist_creation_confirmation_refresh_refusal_and_late_widgets -- --ignored --test-threads=1
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' \
  cargo test --lib --all-features --locked \
  native_workspace_full_shell_adaptive_pane_selection_navigation_and_cleanup -- --ignored --test-threads=1
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' \
  cargo test --lib --all-features --locked \
  native_persistent_session_restore_settings_forget_and_close -- --ignored --test-threads=1
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' \
  cargo test --lib --all-features --locked \
  native_catalog_cache_provenance_interleaving_and_retired_replies -- --ignored --test-threads=1

GTK_A11Y=none GSK_RENDERER=cairo GSK_DEBUG=full-redraw dbus-run-session -- \
  xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' \
  cargo +stable test --lib --all-features --locked \
  native_cover_metadata_http_cache_and_render_lifecycle -- --ignored --test-threads=1
GTK_A11Y=none GSK_RENDERER=cairo GSK_DEBUG=full-redraw dbus-run-session -- \
  xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' \
  cargo +stable test --lib --all-features --locked \
  native_visible_cover_retries_after_worker_queue_saturation_without_remap -- --ignored --test-threads=1
```

Repeat the GTK selectors with `GDK_SCALE=2 GDK_DPI_SCALE=1` and a 3840×2160
screen for actual 2× coverage. Preferences component input tests use real
mouse/keyboard with synthetic snapshots/callbacks; separate settings disk tests
cover roundtrip/restart, invalid/corrupt files, permissions and superseded saves.
These are different checks, not a synthetic callback masquerading as a successful
system-keyring lifecycle. The playlist dialog fixture uses a loopback server to
check confirmation, read-only refresh after uncertainty, refusal and late-widget
cleanup; it does not create a real playlist. Current executed results belong in VALIDATION and the
source-matched Draft PR.

The full-shell fixture checks the production adaptive split, actual pane/row
actions, keyboard focus and an Openbox-managed maximized window. The integrated
session fixture injects protected storage and loopback Music/generated MP3,
checks restart/forget/late work and confirms actual GStreamer Playing survives
remember-off and ordinary cache clearing. Neither uses a personal account or
proves a real KWallet/Secret Portal lifecycle.
The catalog fixture checks visible per-view cache provenance, background reply
interleaving and rejection of retired clear/disable replies; placeholder content
is synthetic, not evidence of live provider layout or access.
The cover fixtures start from Music-shaped metadata and a counted loopback image
server, use the ordinary loader/decode path and inspect native GTK display. They
also exercise bounded-worker saturation without requiring a widget remap. Repeat
these two selectors with `cargo +1.98.1` and at 2× as above. Their generated pixels,
synthetic account and isolated display do not establish real Fedora cover display
or image-cache reuse in the owner's account.

## Local OAuth diagnostic (Gate B1)

This existing developer diagnostic is unchanged and is never part of normal
GUI sign-in. Its local manual URL transfer does not configure the new automatic
Cassette session path or change its Music validation result.

Run this experiment **on the owner's local Fedora desktop**, from a trusted,
interactive terminal. Do not run it in Codex Cloud, CI, a remote support session,
a terminal recorder or a shared/logged console. Build the binary first:

```sh
cargo build --workspace --locked
ulimit -c 0
./target/debug/karin --oauth-diagnostic
```

The command takes no credential arguments and reads no credential environment
variables or configuration files. Supply only Karin's own **Client ID** when
prompted locally; input is hidden. Never supply a Client secret, account password,
OAuth access/refresh token or credentials of another application. The four scopes
requested are `music:api-public`, `music:major-public-api`, `login:info` and
`login:avatar`; they must be assigned to this exact user-authorization app.
Its registered test redirect must be exactly
`https://oauth.yandex.ru/verification_code`.

The tool creates a fresh cryptographically random verifier and state, uses S256,
and opens the authorization request through GIO's Linux default-browser handler.
It does not print the authorization URL or embed a browser. Check that Yandex's
consent screen names **Karin** and the expected permissions. Complete consent in
the system browser. Once it reaches the registered verification page, copy the
**complete URL from the browser's address bar directly into Karin's hidden local
redirect prompt**. Keep it out of the shell, chat, support logs and files: it
contains the one-time code and returned state. The diagnostic expects the exact
`https://oauth.yandex.ru/verification_code` origin/path, extracts the opaque code
and validates `state` against this attempt before exchanging it. The observed
provider-added `cid` is accepted without assigning it a meaning. Missing or
duplicate response fields, unexpected parameters, fragments, wrong origin/path
and invalid state are rejected locally. A code-only paste, OAuth token or
implicit-flow URL is not an alternative. Blank input, Escape, Ctrl-C or Ctrl-D
cancels; each hidden prompt has a five-minute input deadline. If the browser
fails to launch, rerun after correcting the desktop default browser rather than
copying credentials into another tool. An interactive controlling terminal is
required; stdin, including any pipe or redirection, is never read for credentials.
Without a controlling terminal it refuses before browser/network activity. Handled
cancellation restores terminal settings. External termination/suspension can
interrupt cleanup; recover the terminal with `stty sane` if needed.

The owner observed an alphanumeric code and echoed `state` in this browser
redirect on Fedora. Authorization codes are bounded, nonempty opaque values;
there is no digit-count or numeric-format assumption, and the tool does not trim
or normalize a decoded code. The full pasted URL is limited to 4096 bytes and
the decoded code to 2048 bytes; control characters are rejected. Successful
returned-state validation is reported as `Returned state: ValidatedRedirect`.
S256 PKCE also binds the exchanged code to this process's verifier.

This is **manual URL transfer**, not an automatic callback listener, production
application sign-in or persistent account session. A production Linux callback
still needs a separately supported design and acceptance. The initial tool's
seven-digit validation rejected the owner's code before any token exchange;
discard that previous/exposed/expired code and start a fresh local run with the
corrected binary. Code expiry, state mismatch and failed/denied consent require
a new flow, not a different OAuth identity or bypass.

After a secretless initial code exchange, the tool performs these separate checks:

1. `GET https://login.yandex.ru/info?format=json`: require the returned `client_id`
   to match the locally entered Client ID and observe whether identity is present.
   Profile fields and avatar are neither displayed nor fetched separately.
2. `GET https://api.music.yandex.net/account/status`: observe authenticated-account
   presence and `serviceAvailable` independently of ID success.
3. `GET https://api.music.yandex.net/account/about`: make one independent read-only
   request with the same Karin token after successful ID-client/account checks,
   even if status returned HTTP 403, failed on the network or had an unsupported
   response. Report its HTTP status and fixed error category separately as
   `Music /account/about`. HTTP 200 is accepted only for a bounded JSON object
   containing an object `result`, without a top-level `error`; no personal fields
   are retained or interpreted. This is a minimal diagnostic envelope check,
   not a verified about schema or evidence of an authenticated Music account.
4. `GET /search?text=Beethoven&type=track&page=0&nocorrect=true` on the same
   Music host, only after status establishes an authenticated Music account.
   About success cannot unlock the catalog after failed status; about failure
   does not suppress the catalog after valid status. The query reports HTTP
   status and a recognized result-object shape without printing catalog or
   account content. `serviceAvailable=false` remains visible; it does not
   prevent this read-only catalog diagnostic once an account is present.

The status line is labeled `Music /account/status`. Neither Music method runs
if ID binding/account checks fail. Each method keeps its own outcome; about never
changes the account/service facts obtained from status. The overall diagnostic
passes only when every planned stage, including about, succeeds. Even that result
would not establish production sign-in or stream rights.

Only sanitized HTTP statuses, boolean/unknown facts and fixed error categories
are printed. A missing, restricted or malformed result remains visible; the tool
does not retry with another client, region or endpoint. A token's optional `scope`
response can reveal a smaller grant, but its omission is not an independent scope
introspection result. No stream URL, audio bytes or protected content is requested.

For Music HTTP 403, only the exact allowlisted identifier `missing-required-scopes`
in a top-level string `error` or nested `error.name` selects the fixed Music
scope-error category. The tool does not classify free-form messages such as
`API access restricted`, even when they resemble a known condition. Unknown or
malformed responses remain `Forbidden`;
the tool never prints raw JSON, error messages/descriptions, personal fields,
request IDs, headers or URLs. Forbidden Music status leaves account presence and
service availability `unknown` and skips the catalog query, while the independent
about probe still runs. Both Music methods and the catalog share this safe error
classification. Classification uses each response already received and adds no
retry or fallback request; about is a separately planned probe, not a way around
status refusal. Both methods retain the five-second connect limit, fifteen-second
total deadline and 1 MiB body ceiling; redirects and retries remain disabled.

Tokens are kept only in process memory until the diagnostic ends. Owned sensitive
buffers are zeroized on release; this does not guarantee erasure of transient
HTTP/TLS-library or operating-system copies. `ulimit -c 0` disables core dumps
for the invocation; it does not disable swapping or external process inspection.
There is no
refresh, token file, keyring write, automatic account login or GUI session.
Secret Service presence detection alone is not a verified credential lifecycle.
Closing the process drops local token state; it does not revoke Yandex's server
consent. If appropriate, remove the grant through the account's own Yandex
application-permission settings. The browser has its own history/session storage;
do not publish its URL, screenshots, code page or account data.

A successful local run must distinguish code issuance, token issuance/client
binding, actual Music account access and catalog access. Registration alone or
Yandex ID success does not establish the latter two. Share only the sanitized
report, never codes, tokens, verifier, Client secret, private JSON or screenshots.
The owner has since confirmed fresh state validation, token HTTP 200 and Yandex
ID HTTP 200 with a matching Karin client on Fedora. Music status still returned
HTTP 403 in the DIRECT-path repeat; a separate anonymous curl control returned
HTTP 200. These are distinct request contexts, not proof of identical egress or
Music authorization. The owner subsequently observed generic HTTP 403 for both
status and about with TUN on and off. The actual cause remains unknown; share
only the separate sanitized results, never bodies or credentials.
Do not repeat OAuth solely to recheck VPN, change routes, region or application
identity, or publish raw responses. Safe classification
does not bypass the denial. See
[API feasibility](API_FEASIBILITY.md) and [validation](VALIDATION.md).

## Codex Cloud / Debian 13

The prepared cloud environment retains its setup assets outside the repository
at `/workspace/karin-environment`. The Rust toolchain lives in its `cargo/` and
`rustup/` directories; native dependencies live in a Docker image. Use the wrapper
so Cargo sees GTK and GStreamer:

```sh
/workspace/karin-environment/run-auth cargo build --workspace --locked
/workspace/karin-environment/run-auth cargo test --workspace --locked
/workspace/karin-environment/run-auth sh
```

The wrapper's working directory is `/workspace/karin`. It mounts the current CA
bundle read-only and uses the platform proxy; neither verification nor the proxy
should be bypassed. The agent must follow the cloud environment startup
instructions if the Docker image needs restoring. Cloud tasks are already
isolated; use the existing checkout instead of creating another worktree unless
explicitly requested.

For the pinned compatibility toolchain, install it alongside stable and select it
per command; the wrapper does not change the default toolchain:

```sh
/workspace/karin-environment/run-auth rustup toolchain install 1.98.1 --profile minimal \
  --component rustfmt --component clippy
/workspace/karin-environment/run-auth cargo +1.98.1 build --workspace --locked
/workspace/karin-environment/run-auth env RUSTUP_TOOLCHAIN=1.98.1 \
  python3 packaging/flatpak/prepare-source.py
```

`RUSTUP_TOOLCHAIN` also selects the Cargo subprocesses used by source preparation
and the dependency-license inventory.

## Verification

Run from the repository root (through the wrapper in the prepared cloud).
Default and all-feature checks require WebKitGTK6 as described above:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --all-features --locked
cargo build --workspace --locked --all-features
cargo build --workspace --locked
./target/debug/karin --audio-self-test
./target/debug/karin --playback-self-test
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  xvfb-run -a ./target/debug/karin --smoke-test
```

`--audio-self-test` checks a finite synthetic pipeline through `fakesink`.
`--playback-self-test` instead generates a temporary WAV and exercises the real
`playbin` controller, including transport, volume, seeking, EOS and cleanup.
Both run without music, physical audio output, accounts or network access.
The GTK smoke mode starts the actual application and checks its lifecycle and
responsive layout. CI runs the checks on Ubuntu 24.04 with its native development
libraries; a CI definition alone is not evidence that a remote run passed. Read
the executed results in [VALIDATION.md](VALIDATION.md).

CI runs both current stable and the pinned minimum-version toolchain. To reproduce
the Rust 1.98.1 lane locally, install its Rustfmt and Clippy components, then run:

```sh
cargo +1.98.1 fmt --all -- --check
cargo +1.98.1 clippy --workspace --all-targets --locked -- -D warnings
cargo +1.98.1 clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.98.1 test --workspace --locked
cargo +1.98.1 test --workspace --all-features --locked
cargo +1.98.1 build --workspace --all-features --locked
cargo +1.98.1 build --workspace --locked
cargo +1.98.1 fetch --locked
cargo +1.98.1 build --release --offline --locked --no-default-features --features cassette-auth
./target/release/karin --audio-self-test
./target/release/karin --playback-self-test
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' ./target/release/karin --smoke-test
```

The release command matches the Cargo command in the Flatpak manifest. Fetching
populates the local Cargo cache first; a generated Flatpak source snapshot instead
uses its vendored sources. Neither check is a full SDK/sandbox package build.
Keep `Cargo.toml`'s minimum version and the pinned CI lane aligned, and verify code
and locked dependencies before raising that minimum. Never bypass Cargo's
version check with `--ignore-rust-version`.

The owner has confirmed the v0.3 native runtime and final tested Flatpak
scenarios at `0232548` on Fedora 44/KDE/Wayland/PipeWire; the supplied scope is
recorded in VALIDATION. Repeat manual checks for later runtime revisions must
cover real system-output playback, pause/resume, seeking, volume,
EOS/error recovery and closing during playback. Visual acceptance must check
window/title controls, resize/maximize behavior, HiDPI, keyboard/focus,
accessibility and both themes under [the UI quality gate](UI_QUALITY_GATE.md).
A headless check does not establish those properties on Fedora.

For v0.4's matched baseline/current visual evidence, follow
[IMPLEMENTATION_REVIEW.md](design/v0.4/IMPLEMENTATION_REVIEW.md). Run
`scripts/capture-native-music-baseline.sh build/ui-captures` **last among Cargo
steps**, after all current-source checks and captures: rebuilding the archived
baseline uses the shared target and must not replace the current evidence
binary. The helper records baseline/harness/executable fingerprints in
`comparison-info.txt`; a subsequent current-source build must rebuild its crate.

## Developer playback diagnostics

Open the developer playback section on the Diagnostics page. It creates a quiet
synthetic WAV in a temporary file and uses the same controller as the playback
self-test. Choose silent `fakesink` output for device-independent checks, or
explicitly choose system output when testing audibility on a desktop. Prepare
the source, then exercise play, pause/resume, stop, volume and seeking when the
source reports it is seekable. The controls do not select or resolve Yandex Music
content, and the temporary media is not shipped in the repository.

Inspect asynchronous state, position, duration and errors as well as listening.
`fakesink` success proves decoding and lifecycle behavior for the generated WAV;
it does not prove PipeWire output, support for other codecs or service streams.
Music authorization/delivery evidence and the independent developer diagnostic
are recorded in [API_FEASIBILITY.md](API_FEASIBILITY.md); current owner acceptance
and remaining acceptance boundaries are in [VALIDATION.md](VALIDATION.md).

For a substantial UI change, also run the screenshot matrix and review it under
[UI_QUALITY_GATE.md](UI_QUALITY_GATE.md). With the capture tools installed:

```sh
scripts/capture-ui.sh build/ui-captures
```

The prepared cloud visual image provides these tools through
`/workspace/karin-environment/run-visual`. The script uses its own Xvfb/Openbox
session and captures Karin only; it must not be pointed at a personal desktop.
Inspection of the images remains required even when the script exits successfully.

## Flatpak foundation

The initial manifest is
[`packaging/flatpak/io.github.tracerxbrhd.Karin.json`](../packaging/flatpak/io.github.tracerxbrhd.Karin.json).
It targets GNOME 50 and the matching Freedesktop 25.08 Rust SDK extension. This
keeps the GTK/Libadwaita runtime independent of the user's desktop environment.
It has no host filesystem permission or Secret Service access grant. The existing
network/audio and exact MPRIS own-name permissions support the accepted client;
v0.3 adds no broad session-bus or host-filesystem grant. The manifest explicitly passes
`--no-default-features --features cassette-auth`, including WebKit session auth
and excluding the legacy library-only Device Flow module from this artifact. Generated media is created within the
application's own temporary directory, so the player requires no access to the
user's music files.

During Fedora acceptance, the GNOME 50 SDK with
`org.freedesktop.Sdk.Extension.rust-stable//25.08` reported Rust 1.98.1. The earlier
Karin requirement of 1.99 rejected that compiler before compilation. The minimum
is now the 1.98 series, with exact 1.98.1 covered by CI; see the executed results in
[VALIDATION.md](VALIDATION.md). Updating the Runtime or native library baselines
is not needed for this metadata mismatch.

Install Flatpak Builder, the SDK/runtime and the Rust extension from the trusted
Flathub remote configured on your machine:

```sh
flatpak install --user flathub org.gnome.Platform//50 org.gnome.Sdk//50 \
  org.freedesktop.Sdk.Extension.rust-stable//25.08
python3 packaging/flatpak/prepare-source.py
flatpak-builder --user --install --force-clean build/flatpak \
  packaging/flatpak/io.github.tracerxbrhd.Karin.json
flatpak run io.github.tracerxbrhd.Karin
```

The preparation script copies the working tree into ignored `build/flatpak-source`
and replaces that generated directory on each run. It excludes Git data, local
environment files and build outputs. It vendors only the versions fixed by
`Cargo.lock`; review the source snapshot before distribution. Cargo's generated
source configuration exists only in that copy. The manifest builds with
`--offline --locked --no-default-features --features cassette-auth`, with no
dependency downloads inside the Flatpak build. The GNOME 50 runtime supplies the
native WebKit engine.
The preparation also runs `packaging/license_inventory.py` in the generated
snapshot and preserves supplied third-party license/notice files in
`third-party-notices/`. The manifest installs these notices, Karin's `LICENSE`
and `docs/LICENSING.md` under `/app/share/licenses/karin`. Review the inventory
and component obligations before distribution; generated notices are not a
complete review of the native runtime or final package.
The SDK's Rust must meet `Cargo.toml`'s minimum version. If a future SDK/compiler
combination falls below it, inspect the SDK's actual `rustc --version` and the
supported extension before changing the minimum or Runtime. Do not suppress the
version check. Karin's own code and original materials
use GPL-3.0-or-later. A distributable package must include the GPL text, preserve
third-party notices and meet corresponding-source and component-specific license
obligations; see [LICENSING.md](LICENSING.md). Source preparation and a successful
build alone do not establish complete distribution compliance or release
acceptance. GPL does not authorize Yandex Music API or content access.

Full Flatpak builds are **not verified in the cloud environment**: Bubblewrap
cannot create the required user namespace inside the nested container. The owner
accepted the merged v0.2 Flatpak build/install/sign-in/library/search/audio/MPRIS/
shutdown. A changed v0.3 package and Secret Portal/backend still need their own
fresh package/build/install and lifecycle acceptance there. Keep sandboxing enabled
and perform packaging acceptance on Fedora or suitable CI.

GNOME 50's Freedesktop SDK base is documented in the upstream
[gnome-build-meta junction](https://gitlab.gnome.org/GNOME/gnome-build-meta/-/blob/gnome-50/elements/freedesktop-sdk.bst).
