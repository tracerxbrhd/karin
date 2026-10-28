<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# v0.1 service-audio delivery review

Reviewed: 2026-10-28. **Service playback remains unavailable in this slice.**
The inspected client does not establish an identity-neutral, permitted audio
delivery path that Karin can implement within this task's restrictions. This is
a specific evidence gap, not a claim that all Music audio is encrypted or that
every independent client is impossible.

## Evidence and limits

The source reference is `okhsunrog/yamu` 0.1.2 at immutable revision
`9803f319b9ea6f81befbc3117e4592132ed07d0e`. This review reads source only: no live
OAuth, Music metadata, delivery-info or CDN request was made in Cloud, no audio
was downloaded, and no credential/signing-key value or protection algorithm is
copied into Karin.

| Observation | What it establishes | What it does not establish |
| --- | --- | --- |
| Owner's Fedora `yamu` Device Flow returned account, search, track and album data; same-token status A/B worked without the Android header | Metadata/account requests work for that experimental token/context | A delivery grant, usable audio URL, stream entitlement or public distribution permission for Karin |
| Own-client Karin PKCE and ID binding succeeded, but both Music account methods returned 403 with TUN on/off | The observed own-client legacy-account path remains refused | The exact server reason, lack of subscription or impossibility of another provider-approved integration |
| `yamu` has a `download_info` / `open_audio_stream` API | The library contains a delivery implementation | A permitted contract for Karin or a source that can be resolved without the excluded mechanisms |

Owner observations are recorded in [Issue #5's final own-client comparison][own]
and the [working-client][working] / [header A/B][ab] reports. They are not Cloud
account tests. [Issue #7][issue] explicitly separates metadata access from audio
delivery and prohibits extracted protection material or an access workaround.

## Concrete implementation blocker

The reviewed [`download_info` implementation][negotiation] obtains delivery
information through a separate `/get-file-info` negotiation. Its source explicitly
identifies a signing constant as extracted from an official Music client, and the
request presents a separate official Web client identity and browser-origin
headers. Karin cannot treat this as its own supported, identity-neutral API
contract or port the extracted signing material under the task's constraints.

The [`DownloadInfo` model][delivery-model] can contain URLs and an optional
decryption key. The [`downloader`][downloader] handles both ordinary transfers
and a key-dependent encrypted path. An optional key means **not every response
is necessarily encrypted**. Conversely, a `raw` transport label, a codec name,
an available-track flag or a model that permits an unencrypted response does not
remove the preceding negotiation/identity question. No inspected track/catalog
method supplies a separately confirmed unsigned, permitted audio source.

The reviewed official integration/terms evidence in
[API_FEASIBILITY.md](API_FEASIBILITY.md) also contains no validated delivery
contract for Karin. Code licenses and a user's subscription do not supply that
contract. This is an engineering gate, not a legal determination about another
application. Source compatibility and the owner's metadata results cannot
resolve the gate by themselves.

## Behavior for this vertical slice

- Implement useful session, account, library, search and detail interactions
  independently of stream delivery. Preserve own-client PKCE as a separate
  diagnostic. An experimental session is not production integration approval.
- Do not request delivery information, follow service/CDN URLs, add official
  client headers, import media signing/decryption modules or resolve a remote
  `PlaybackSource` from metadata. Do not download/cache service audio.
- A real catalog track may be selected and inspected, but playback must report
  the missing permitted source explicitly. Do not turn the existing generated
  WAV diagnostic into that track or change its title to imply real playback.
- Keep the existing GStreamer controller and its actual local-media tests.
  Queue/selection state can be exercised independently, but audible service
  playback, EOS from service media and MPRIS playback claims remain unverified.

The remaining evidence is a provider-supported, identity-neutral delivery
mechanism for the intended client and an appropriately entitled owner's consented
Fedora test. It must not require an extracted signing secret, decryption workaround
or false application identity. If such a mechanism becomes available, a separate
change can validate source authorization/expiry/redirect policy, pass only an
approved resource to GStreamer, exercise real pause/seek/queue/EOS/cleanup and
report audibility on Fedora. No such success is claimed here.

Useful API model/endpoint ideas can still be reused without importing the
delivery implementation; see [the Rust-client review](RUST_API_CLIENT_REVIEW.md).
This document contains original analysis and source links rather than copied
third-party source or constants. Any later source adaptation must retain its
applicable third-party notices and license.

[negotiation]: https://github.com/okhsunrog/yamu/blob/9803f319b9ea6f81befbc3117e4592132ed07d0e/src/client/downloads.rs#L16
[delivery-model]: https://github.com/okhsunrog/yamu/blob/9803f319b9ea6f81befbc3117e4592132ed07d0e/src/models/download.rs#L156
[downloader]: https://github.com/okhsunrog/yamu/blob/9803f319b9ea6f81befbc3117e4592132ed07d0e/src/downloader.rs#L287
[own]: https://github.com/tracerxbrhd/karin/issues/5#issuecomment-6470853664
[working]: https://github.com/tracerxbrhd/karin/issues/5#issuecomment-6471162750
[ab]: https://github.com/tracerxbrhd/karin/issues/5#issuecomment-6471631991
[issue]: https://github.com/tracerxbrhd/karin/issues/7
