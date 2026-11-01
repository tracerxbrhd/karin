<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin

An independent native Linux client for Yandex Music, built with Rust, GTK4,
Libadwaita and GStreamer. Karin is not affiliated with Yandex.

**v0.4 — Native Music** is the current UI iteration in existing
[Draft PR #15](https://github.com/tracerxbrhd/karin/pull/15): a compact expandable
rail, cover-led music workspace, one nonmodal Track Info / Queue panel and a
centered Player Bar with real local Shuffle/Repeat modes. The owner's
[complete redesign brief](docs/design/v0.4/CODEX_TASK.md) supersedes the initial
visual direction: `a9eed45` passed cloud checks but its Fedora UI was not
accepted. The [current specification](docs/design/v0.4/DESIGN_SPEC.md) keeps
visible prose concise and preserves the accepted sign-in, MusicService,
StreamResolver, GStreamer player, contextual queues, protected session/cache
and MPRIS. Implementation checks and Fedora visual acceptance remain separate;
the [SVG references](docs/design/v0.4/README.md) are illustrations, not GTK captures.
See the [fresh actual GTK before/after and motion gallery](docs/design/v0.4/evidence-redesign/README.md)
for the current redesign.

**v0.3 — Seamless Experience** is merged through
[PR #14](https://github.com/tracerxbrhd/karin/pull/14) at `5e85e8d`, delivering
protected session persistence, bounded metadata/artwork caching and the accepted
functional workspace on which v0.4 is based.

The owner accepted the merged v0.2 baseline on Fedora 44 / KDE Plasma / Wayland /
PipeWire, including Flatpak: **17 PASS, 0 FAIL, 1 N/D** (hardware media keys).
That includes real sign-in, library/search, actual Music playback, queues,
transport/seek/volume, KDE Media Widget and shutdown. On **2026-10-31** the owner
accepted v0.3 runtime **`02325489d23ead9da4471de9cc35e2c063ac8bab`**: persistent
sign-in/restart, real covers in lists/Player Bar/right pane, real Music playback,
pane mechanics and KDE cover/metadata. Final Flatpak authorization, playback
and system-integration checks were reported successful. The pane's visual design
was not accepted; the separate v0.4 iteration addresses it and needs new owner
visual acceptance.
This is functional-prototype acceptance, not a public release. See
[validation](docs/VALIDATION.md) and the
[current v0.4 Fedora procedure](docs/BUILDING.md#v04-native-music-acceptance-on-fedora).

## Ordinary sign-in and protected session

Click **Войти**, use the actual Yandex page in the isolated native WebKitGTK6
dialog, and wait for Music account validation before library access. There are
no Client ID/secret fields, application-registration instructions, Device Flow
codes, pasted tokens or manual URL transfer in this journey. The established
Cassette-compatible session provider is retained; the application remains native
GTK/Libadwaita and does not read passwords or import a browser profile.

v0.3 separates live session state, protected credential I/O and non-secret
preferences. Native builds use the available Secret Service; Flatpak selects
the available Secret Portal/backend and a private authenticated encrypted token
envelope. A saved credential is validated with MusicService before becoming an
authenticated session. Remember-login is effective by default only when protected
storage is supported. Missing, locked, cancelled or broken storage remains
explicit and permits ordinary session-only login without plaintext fallback.
Active cleanup I/O settles first; if it fails, memory-only login can continue
with persistence paused. A successfully saved forget barrier suppresses old
restoration across restart. If that barrier itself cannot be written, restart
protection remains explicitly unconfirmed; retry cleanup before relying on it.

Turning **Запоминать вход** off removes the saved credential while the current
in-memory session may continue. A previously enabled preference can be turned
off even if the backend becomes unavailable; Karin records the forget intent
and exposes unresolved deletion. Enabling it requires supported protected
storage. Explicit logout retains the safe default
**Отмена** confirmation, then stops playback, clears personal presentation and
persistent metadata/covers, and verifies saved-credential deletion. Unknown
cleanup stays visible/retryable; the pending-forget state pauses persistent
credentials/cache until resolved. Normal window close may retain a confirmed remembered
session and permitted cache. Portal forgetting removes Karin's token envelope,
not the portal's master key or Yandex's remote grant. No refresh or server
revocation mechanism is claimed.

The owner's persistence/restart and Flatpak results do not identify every
keyring/portal backend or separately confirm its deletion/failure matrix. A
running service, available library, source listing or synthetic fixture alone
does not prove those additional lifecycle cases. See
[ADR-0018](docs/adr/0018-persistent-session-and-settings.md).

## Music workspace

- The accepted compact expandable icon rail/drawer, circular account avatar,
  external account-page action, four-section unified search and persistent
  player bar are retained. Appearance and other real preferences persist.
- A track row starts its exact loaded-context occurrence by mouse, Enter or
  Space. Liked tracks, albums, playlists and search retain their real ordering,
  duplicates and partial-collection boundary within 500 queue positions. Manual
  appends supplement the context; Next/Previous/EOS use that same queue. Shuffle
  plans exact occurrences with bounded played history, not unique IDs; repeat
  cycles Off → Queue → One. Repeat One applies to natural EOS while explicit
  Next/Previous still navigate. Modes survive an ordinary clear or new context
  and reset on logout/close. Details remain a separate action.
- Adaptive track information presents actual large cover/title/artist/album,
  selected/playing state and supported context actions. Album/artist links use
  verified identities rather than guesses from display names. Opening, closing
  or resizing information must preserve playback and context. v0.4 separates
  explicit open intent from responsive presentation: outside clicks/navigation
  retain the view, explicit close dismisses it, and a new current queue occurrence
  updates it in place. Details may inspect another track without playing it;
  the next actual current-item change resumes following. See
  [ADR-0022](docs/adr/0022-universal-music-panel.md). The same pinned surface
  switches between Track Info and Queue without closing/reopening; Ctrl+J and
  the Player Bar queue button replace the former primary queue destination.
  Full-size background, header, artwork and text translate together using a
  single native animation; compact Back and explicit close remain available.
- Playlists use adaptive native cover tiles, with real optional counts, readable
  long names, missing-cover handling and loading/empty/error states. Creation
  uses one explicit request and independent server confirmation; an ambiguous
  write is not a local success or an automatic retry. Broader unsupported edits
  are not implied by a Create tile.
- Search validates minimal own track IDs, then uses one bounded canonical batch,
  preserving order, duplicates, unavailable/missing records and provider paging.
  Album/artist/playlist sections, Show All and Retry remain independent.
- The existing guarded full-MP3 resolver feeds an opaque short-lived HTTPS source
  to the existing GStreamer worker. Actual snapshots drive play/pause/resume,
  progress/Seek, Volume, queue and cleanup. Stop remains an internal and MPRIS
  command and has no visible Player Bar button. OAuth/cookies never go to the
  descriptor/CDN; signed addresses and music files are not persisted.
- MPRIS 2.2 controls that same player through GIO session D-Bus. Known metadata,
  actual position/capabilities, confirmed seeks, volume, writable Shuffle and
  LoopStatus, and private temporary cover-only PNGs are exported; tokens, cookies, account IDs and media URLs are
  not. The exact Flatpak MPRIS own-name permission is retained. Baseline KDE
  Media Widget/Flatpak integration passed; v0.3 KDE real cover/metadata and final
  Flatpak system integration are owner-confirmed. Physical media keys were not
  individually reported.

My Wave, similar-track feeds and offline audio are enabled only with their
complete real contracts/rights lifecycle. Unsupported functions stay hidden;
there is no fabricated recommendation catalog, renamed ordinary queue or inert
Offline switch. The [Cassette capability audit](docs/CASSETTE_PARITY.md) separates
source implementations, Karin features, fixture checks and actual acceptance.

## Data cache and settings

Metadata and explicitly cacheable normalized covers use a private account-scoped
XDG cache, independent of GTK. Defaults are enabled, **256 MiB**, metadata TTL
**15 minutes** and artwork TTL **7 days**, with shorter server freshness and
explicit HTTP restrictions respected. The configurable limit is **16 MiB–4 GiB**.
Records have bounded size/counts, integrity checks, atomic writes, corruption
recovery and LRU-like eviction. No raw provider response, token, cookie, signed
media URL or audio belongs in the cache.

**Настройки → Хранилище** exposes policy, measured usage and clear progress/errors.
Cache-off stops persistent content reads and new writes; existing files remain
until explicit clear. Clear must preserve credentials, current audio/context,
displayed textures and the live MPRIS cover. Account/policy generations discard
obsolete work. Explicit refresh and current membership/write confirmation use
the server; saved/stale fallback metadata is labelled and never authenticates
an account or claims current rights. See [ADR-0019](docs/adr/0019-bounded-account-cache.md).

Persistent audio is a separate gate. A playable MP3 and official-client download
feature do not establish Karin's third-party storage permission or enforceable
offline-entitlement lifecycle. The exact evidence and lawful prerequisites are
recorded in [AUDIO_CACHE_REVIEW.md](docs/AUDIO_CACHE_REVIEW.md). Metadata/artwork
persistence does not imply offline Music playback.

## Build and check

Install stable Rust (minimum series 1.98; checks pin **1.98.1**) and the native
packages in [BUILDING.md](docs/BUILDING.md), then:

```sh
cargo build --workspace --locked
cargo run --locked
```

Default builds include the established `cassette-auth`/WebKit provider. The
minimal no-default-feature build deliberately omits it; library-only legacy
Device Flow and the independent local own-client PKCE diagnostic are not the
ordinary login path. The earlier own-client ID 200 / Music 403 observation is
historical and is not reused as the normal provider or a diagnosis of unrelated
UI failures.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --all-features --locked
cargo test --workspace --no-default-features --locked
cargo build --workspace --locked --no-default-features
cargo build --workspace --locked
./target/debug/karin --audio-self-test
./target/debug/karin --playback-self-test
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- xvfb-run -a ./target/debug/karin --smoke-test
```

Use the prepared cloud wrapper for native dependencies. Rust 1.98.1 additionally
checks a fetched/vendored `--release --offline --locked` build matching the
Flatpak command. Tests use only synthetic accounts, isolated files/D-Bus and
loopback services. Generated audio checks actual decoding/lifecycle, not Music
rights, PipeWire audibility or real account acceptance. Smoke/capture modes
inject offline state and must not restore a personal saved session/cache.

GNOME 50 and the Freedesktop 25.08 Rust extension remain the Flatpak foundation.
The owner reported successful final v0.3 Flatpak checks for the accepted runtime;
the exact supplied scope is recorded in validation. Future changed packages
still need their own regression. Source preparation and native offline
compilation are not a full sandboxed Flatpak build. Xvfb does not pass the
Fedora Wayland visual gate.

## Documentation and release scope

- [Architecture and concurrency](docs/ARCHITECTURE.md)
- [Fedora, cloud and Flatpak instructions](docs/BUILDING.md)
- [Validation evidence](docs/VALIDATION.md) and [known limitations](docs/LIMITATIONS.md)
- [Native Music design inputs](docs/design/v0.4/README.md) and [UI quality gate](docs/UI_QUALITY_GATE.md)
- [v0.3 cloud screenshots](docs/screenshots/v0.3/README.md) and [historical UI galleries](docs/ui/README.md)
- [Audio-cache rights and offline lifecycle](docs/AUDIO_CACHE_REVIEW.md)
- [Authentication contract](docs/AUTHENTICATION_REVIEW.md), [API evidence](docs/API_FEASIBILITY.md) and [delivery review](docs/STREAM_DELIVERY_REVIEW.md)
- [Cassette capability audit](docs/CASSETTE_PARITY.md), [roadmap](docs/ROADMAP.md) and [ADRs](docs/adr/)
- [License scope and dependency review](docs/LICENSING.md)

The prototype adapts Cassette's published shared registration/session contract
with GPL notices preserved. Its source-license permission does not establish
Karin's public OAuth-registration/service-distribution permission or Linux
embedded-auth approval; no blanket service ban is inferred either. No new
identity, DRM bypass, extracted delivery key or copyrighted recording is added.
The owner explicitly authorized PR #14's completed squash integration after
final review and successful CI. It does not authorize merging Draft PR #15 or
constitute public-release approval.

Karin's own code and original materials use **GNU GPL-3.0-or-later** unless a file
states otherwise. [LICENSE](LICENSE) retains the standard full GPLv3 text and
third-party components retain their notices. GPL grants software rights, not
Yandex API, registration, trademark or content permission, and does not replace
the service's terms.
