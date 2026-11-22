<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Known limitations

The owner has completed Fedora acceptance of v0.8 functionality.
[VALIDATION](VALIDATION.md) separates that report from automated evidence and
records historical failures. No public release or universal backend/hardware
certification is implied.

## Service, content and distribution

The Music API is unofficial and can change. A valid account, subscription or
cached metadata does not authorize every resource. Unknown delivery schemas,
origins, DRM/protection and unsupported formats fail explicitly. Karin does not
import official-client sessions, caches or extracted signing keys.

GPL and third-party source provenance are separate from OAuth-registration,
service/content and public-distribution rights. Those distribution conditions
remain unresolved in [#5](https://github.com/tracerxbrhd/karin/issues/5) and
[#22](https://github.com/tracerxbrhd/karin/issues/22). See
[licensing](LICENSING.md), [authentication](AUTHENTICATION_REVIEW.md) and
[stream delivery](STREAM_DELIVERY_REVIEW.md).

`0.8.0-rc.1` is a verifiable engineering candidate, not an authorized public
release. The [completed owner-send request](../packaging/release/YANDEX_REQUEST_RU.md)
has not been sent. Flathub has a separate
[AI-manifest/submission policy](https://docs.flathub.org/docs/for-app-authors/requirements#generative-ai-policy):
these AI-assisted standalone manifests are not claimed submission-eligible.
That store-specific condition does not prevent preparing or testing an independent
source-based Flatpak bundle. No tag, release or store submission is authorized.

Yandex supplement text needs explicit rights and full text; it has no timing.
Synchronized LRCLIB text depends on external catalog availability and metadata
matching. External lookup needs explicit consent or the separately saved,
default-off automatic preference, which can be revoked. Text stays in memory.
Source software licenses do not license every lyric for redistribution.
[ADR-0034](adr/0034-bounded-track-lyrics-and-similar.md) defines these contracts.

## Catalog, writes and playback

- Ordinary views stop at 2,000 positions/100 pages. Search-driven indexing of
  supported liked/playlist collections admits at most 20,000 original positions/
  4 MiB, 50-position batches and 200-row result windows, with a final membership
  recheck. Larger, changed or incomplete collections are labelled honestly.
  Added-date sorting requires valid occurrence timestamps; no startup/full-library
  scan or persistent second database is provided.
- Playback contexts contain at most 500 occurrences. Search, sort, index growth
  and server edits never rewrite the active queue. Wave reorder/Play Next is
  unsupported to preserve its batch and feedback protocol.
- Playlist editor snapshots require complete positions/revision and at most
  2,000 entries. Rename/visibility/delete have no proven server revision CAS;
  preflight/readback cannot exclude every external-writer race. Ambiguous writes
  permit GET-only reconciliation and are never automatically replayed.
- My Wave starts only after explicit **Слушать**. Radio sessions/batch markers are
  not persisted or restored; local retirement sends no `radioFinished` event.
  A request already sent cannot be recalled. Feedback errors remain visible.
- Only independently confirmed Dislike addition on the same current occurrence
  invokes Next. Unknown Dislike remains hidden; a successful independent Like
  read can preserve Like/Unlike without guessing Dislike or recommendation effects.
- The HTTPS descriptor/CDN allowlist is intentionally narrow. Admission expiry
  does not interrupt an already playing track. Explicit Resume can renew once for
  the same occurrence and restore a confirmed position. Network/access failures
  and unseekable restoration remain honest errors; no periodic Pause renewal occurs.
- Shutdown is cooperative; bounded deadlines cannot preempt a blocked native
  plugin call, and forced termination cannot guarantee native cleanup.

## Cache and protected storage

One shared cache defaults to 2 GiB with six quota choices including Unlimited;
Unlimited still requires disk headroom and bounded records/work. Audio reuse
requires current Music access, matching authenticated offer/variant, integrity
and a fresh private HTTP lease or sufficient revalidation proof. `no-store`,
`no-cache`, stale policy and explicit restrictions remain effective. A saved icon
means complete local presence, not a guaranteed hit or offline availability.
Cache failure preserves admitted online playback. Downloads, mass prefetch and
Offline Mode are not implemented. See [AUDIO_CACHE_REVIEW](AUDIO_CACHE_REVIEW.md).

Known metadata refusals fence cached replies before disk work. If disk failure
prevents durable refusal recording, inherited metadata starts Stale after
restart; known tombstones are never served. The 8,192-key/account fence limit
pauses persistence until successful Clear without stopping live playback.

Secret Service/Secret Portal availability depends on the desktop/backend.
Missing or locked storage allows memory-only login without plaintext fallback.
Restoration always needs fresh Music validation; no automatic token refresh or
remote revocation is claimed. Uncertain forgetting/cache purge retains a durable
barrier and visible Retry; failed barrier persistence leaves restart protection
unconfirmed. Portal forgetting removes Karin's encrypted envelope, not a portal
master key. Ordinary continuation restores IDs/order/position without autoplay;
Stop/forget/logout remove it and Wave never persists.
[ADR-0018](adr/0018-persistent-session-and-settings.md) defines storage lifecycle.

## Presentation and evidence

System uses Libadwaita's supported preferences, not KDE Qt/Breeze CSS. Karin Style
is unavailable until real designer materials exist. High Contrast uses opaque
semantic surfaces; Reduced Motion applies immediate endpoints. The legacy GTK4
`gtk-application-prefer-dark-theme` warning can originate before Karin applies
appearance; the exact source on the owner's desktop remains unestablished.
The optional [read-only check](BUILDING.md#theme-input-check) does not alter settings.

Cloud first-player rendering in [#23](https://github.com/tracerxbrhd/karin/issues/23)
has a measured software-GL boundary: actual llvmpipe context and matching GTK/
Gallium/LLVM stacks locate substantial cold time in lazy native draw-variant JIT,
with much shorter Cairo and repeated-Play controls. No newly measured Karin
production defect has justified a workaround. The remaining costs are not fully
split; closing the issue still needs a disposition accepted by the owner.
[VALIDATION](VALIDATION.md#remaining-evidence-limits) records the bounded numbers,
and [one isolated desktop diagnostic](BUILDING.md#bounded-cold-render-diagnostic)
can check the real platform without repeating full v0.8 acceptance. Cloud results
do not establish physical GPU/Wayland behavior, heard service music or every
optional keyring/portal/media-key scenario. Keep WebKit's sandbox and TLS enabled.
