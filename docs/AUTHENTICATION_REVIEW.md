<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Authentication contract review

Reviewed **2026-10-29** for the owner's [PR #8 correction][correction]. This is
public source/document research. No real authorization, account access, token
exchange or private provider call was performed. Upstream credential/signing
values are deliberately omitted. Source inspection does not establish provider
permission or live acceptance.

## What Cassette actually does

The inspected revisions are Cassette **v0.2.4**,
`fc6c6e689bd0c60fa662b03aa2aa3c05534b4811`, and **main**,
`61eb9c662f7a59e9f5afe059641c4bcfbe32d05f`.

| Stage | v0.2.4 evidence | Current main evidence |
| --- | --- | --- |
| Provider login | [BeginView][old-view] loads `https://oauth.yandex.ru/authorize` with `response_type=token` and a hardcoded existing application identity into WebKit; cookies persist in SQLite | [WebkitAuthDialog][new-view] loads the same identity/flow, writing a temporary cookie DB and moving it to the app's cookie path |
| UI completion | A substring navigation check emits completion; [BeginDialog][old-dialog] closes before backend validation | The same substring check sets success and closes; [Auth][new-auth] then starts backend initialization |
| Token issuance | [YaMClient][old-client] posts `grant_type=sessionid`, hardcoded application ID/secret and host to official OAuth using the [cookie-backed Soup session][old-cookies] | [YaMAPI.Client][new-client] performs the same cookie-backed sessionid grant |
| Music validation | OAuth token plus an Android Music-client header, followed by `/account/about`; the [talker][old-talker] checks Plus and initializes liked playlists/tracks | OAuth token/header, `/account/about` and Plus check, followed by [`/library/all-ids` initialization][new-library] |

The authorize and token-form client identities match, and v0.2.4/main reuse the
same identity; values were compared without retaining them in this document.
This is a cookie-backed sessionid grant, rather than Karin's code/PKCE grant or
extracting an access-token fragment from the WebView. The reviewed chain does
not independently check Yandex ID client binding. Popup closure itself does not
prove Music authorization. Current main's temporary cookie file also does not
by itself establish an isolated ephemeral WebKit network session.

Karin must not adopt the reused identity/secret, sessionid cookie exchange,
substring callback check, navigation-URL logging, persistent cookie/token model
or close-before-validation ordering. Public source/code licensing does not grant
rights to another application's OAuth registration or service access.

## Provider documentation and native callback

- [Yandex code flow][code] explicitly permits the initial token exchange without
  `client_secret` when PKCE and `code_verifier` are supplied. It documents echoed
  state and prefers S256. This establishes generic initial exchange support,
  independently of Music permissions.
- [Yandex registration][register] requires matching registered scheme, host,
  port and path; query/fragment may differ. Its listed platforms are Web,
  iOS and Android. A supported Linux callback and RFC 8252's dynamic-loopback-port
  exception are not established by these docs. The existing verification-page
  registration cannot simply be substituted with an automatic loopback URI.
- [Yandex LoginSDK][mobile] explicitly permits WebView/device browser in its
  iOS/Android SDK context when Yandex apps are absent. A blanket Yandex WebView
  ban would be inaccurate. This does not establish standalone Linux WebKitGTK,
  sessionid-grant or Music-distribution permission.
- [RFC 8252 §8.12][rfc] says native apps **MUST NOT** use embedded user agents;
  the host can inspect credentials and cookies. Ephemeral storage does not change
  that classification. Sections 7.3/8.3 support system-browser loopback return;
  §§8.4/8.9/8.10 require redirect/state validation. These standards do not prove
  Yandex supports a particular callback registration.

Official pages were retrieved successfully on 2026-10-29 and are mutable. Unlike
the Git sources above, they have no immutable commit reference. The review is
bounded to the documents linked here and does not claim universal prohibition
or permission.

## Karin evidence and the concrete remaining gate

The owner's [own-client test][own] passed state, secretless PKCE token issuance
and Yandex ID client/account binding. Music status and about both returned
generic **403 / Forbidden**, with TUN on/off. The exact cause remains unknown.
An [unmodified yamu Device Flow][yamu] accessed account/search/metadata; a
[same-token header A/B][ab] accepted status with or without the Android-client
header. That comparison does not isolate OAuth flow from application/grant
identity, approve reuse of that identity, or explain Karin's 403.

No authorized distributable Music provider path is established. Replacing the
browser presentation supplies no evidence of a different Music result. The
remaining project prerequisites are:

1. Karin's permitted Music application identity/grant and distribution conditions.
2. A provider-supported, registered automatic callback matching the selected
   browser flow; fixed-loopback infrastructure alone does not register one.
3. Provider confirmation for Linux embedded authentication if pursued, with the
   RFC discrepancy explicit; otherwise an explained system-browser action with
   automatic return from the same native dialog.
4. Consented local proof of token binding and Music account acceptance, followed
   by actual library/search, cancellation/logout and token lifecycle checks.

These are project-owner/provider facts, not an end-user registration or secret-
entry workflow. The public provider stays blocked. The implemented native dialog,
generic callback/PKCE code and optional isolated `auth-webkit` view are preparation
with synthetic coverage; Cassette-style live sign-in remains unachieved. Audio
delivery has its own [independent gate](STREAM_DELIVERY_REVIEW.md). See
[ADR-0010](adr/0010-single-purpose-sign-in-and-provider-gate.md) and
[API evidence](API_FEASIBILITY.md).

The optional browser component does not establish RFC 8252 compliance or Linux
provider approval. Its navigation policy checks parsed origins and exact callback
state before consuming a code; it is not a firewall for all page subresources.
WebKitGTK 6's selected navigation API does not reliably expose whether an action
is in the main frame (`frame_name` is not that proof). A response-frame test is
too late to intercept a credential-bearing callback before network dispatch.
This unresolved embedded-provider requirement reinforces the default blocked
provider and the separately registered system-browser candidate; no real
embedded provider is activated by enabling `auth-webkit`.

[correction]: https://github.com/tracerxbrhd/karin/pull/8#issuecomment-6486318822
[old-view]: https://github.com/Rirusha/Cassette/blob/fc6c6e689bd0c60fa662b03aa2aa3c05534b4811/src/widgets/begin/begin-view.vala#L47-L104
[old-dialog]: https://github.com/Rirusha/Cassette/blob/fc6c6e689bd0c60fa662b03aa2aa3c05534b4811/src/widgets/begin/begin-dialog.vala#L30-L38
[old-client]: https://github.com/Rirusha/Cassette/blob/fc6c6e689bd0c60fa662b03aa2aa3c05534b4811/src/client/api/yam-client.vala#L53-L89
[old-cookies]: https://github.com/Rirusha/Cassette/blob/fc6c6e689bd0c60fa662b03aa2aa3c05534b4811/src/client/soup-wrapper.vala#L107-L151
[old-talker]: https://github.com/Rirusha/Cassette/blob/fc6c6e689bd0c60fa662b03aa2aa3c05534b4811/src/client/talkers/yam-talker.vala#L72-L134
[new-view]: https://github.com/Rirusha/Cassette/blob/61eb9c662f7a59e9f5afe059641c4bcfbe32d05f/app/src/ui/webkit/webkit-auth.vala#L31-L79
[new-auth]: https://github.com/Rirusha/Cassette/blob/61eb9c662f7a59e9f5afe059641c4bcfbe32d05f/app/src/ui/auth.vala#L108-L161
[new-client]: https://github.com/Rirusha/Cassette/blob/61eb9c662f7a59e9f5afe059641c4bcfbe32d05f/libtape/src/yandex-music/client.vala#L118-L183
[new-library]: https://github.com/Rirusha/Cassette/blob/61eb9c662f7a59e9f5afe059641c4bcfbe32d05f/libtape/src/yandex-music.vala#L91-L102
[code]: https://yandex.ru/dev/id/doc/ru/codes/code-url
[register]: https://yandex.ru/dev/id/doc/ru/register-auth
[mobile]: https://yandex.ru/dev/id/doc/ru/mobileauthsdk/about
[rfc]: https://www.rfc-editor.org/rfc/rfc8252.html#section-8.12
[own]: https://github.com/tracerxbrhd/karin/issues/5#issuecomment-6470853664
[yamu]: https://github.com/tracerxbrhd/karin/issues/5#issuecomment-6471162750
[ab]: https://github.com/tracerxbrhd/karin/issues/5#issuecomment-6471631991
