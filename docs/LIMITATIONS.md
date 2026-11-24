<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Known limitations

Karin's v0.8 functionality has owner-reported Fedora Native/Flatpak acceptance.
Cloud fixtures do not certify live Music access, every desktop backend or physical
GPU behavior. Version `0.8.0` is prepared for the first stable release; public
publication remains a separate owner decision.

## Service and distribution

Karin is unofficial. Music API schemas, account access, catalog availability and
service restrictions may change. Only supported unprotected MP3 delivery is
admitted; unknown origins/formats/protection fail explicitly. No DRM bypass,
imported official-client sessions or extracted keys are provided.

Source licensing, reuse of the published OAuth registration and service/content
rights are separate. Public integration conditions remain unresolved in
[#5](https://github.com/tracerxbrhd/karin/issues/5) and
[#22](https://github.com/tracerxbrhd/karin/issues/22); see [LICENSING](LICENSING.md).
The [prepared owner request](../packaging/release/YANDEX_REQUEST_RU.md) has not
been sent. Flathub's separate [AI-manifest/submission policy](https://docs.flathub.org/docs/for-app-authors/requirements#generative-ai-policy)
means these AI-assisted manifests are not claimed submission-eligible.

Lyrics availability/matching depend on the provider. Yandex plain text has no
timing; external LRCLIB lookup needs consent and text remains memory-only.
Software licensing does not grant redistribution rights to service material.

## Library, playback and storage

- Ordinary views stop at 2,000 positions/100 pages. Eligible search-driven
  collection indexes admit 20,000 positions/4 MiB; playback contexts contain
  at most 500 occurrences. Incomplete/changed/oversized coverage is explicit.
- Playlist edits require complete bounded snapshots and readback. Metadata/delete
  lack proven server revision CAS; external-writer races cannot all be excluded.
  Ambiguous writes are not replayed automatically.
- Wave starts explicitly and never restores a radio session. Future-order edits
  are unavailable for Wave. Feedback failures retain honest error/recovery states.
- Audio reuse needs current online Music access and fresh HTTP reuse proof.
  Stored means local presence, not a guaranteed hit or offline right. Offline
  Mode, downloads and mass prefetch are not implemented. Cache failure preserves
  admitted playback; Unlimited still requires disk headroom and bounded work.
- Protected persistence depends on Secret Service/Secret Portal. Missing/locked
  storage permits memory-only login. Restoration needs fresh Music validation;
  no automatic token refresh or remote revocation is implemented. Failed forgetting
  remains visible with Retry; ordinary continuation never autoplays.
- Native plugin calls cannot always be preempted; forced termination cannot
  guarantee cooperative pipeline/storage cleanup.

## Desktop and rendering

System follows Libadwaita preferences, not Qt/Breeze CSS. Karin Style is not yet
available. An external legacy GTK dark-theme setting may cause an initialization
warning; [the optional read-only check](BUILDING.md#theme-input-check) changes no settings.

[#23](https://github.com/tracerxbrhd/karin/issues/23) records substantial cold
software-GL time in Mesa llvmpipe/LLVM draw-variant JIT, with shorter Cairo and
repeated-Play controls. Remaining costs are not fully split; the boundary has not
been accepted for physical Fedora/Wayland/GPU. There is no production prewarm or
forced renderer workaround. A [bounded desktop diagnostic](BUILDING.md#bounded-cold-render-diagnostic)
can check the target platform without repeating accepted v0.8 functionality.
