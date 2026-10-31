<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Authentication contract review

Current status, **2026-10-31**: the retained provider's real sign-in, Music
account/library/search and playback are part of the owner-accepted merged v0.2
native/Flatpak baseline (**17 PASS, 0 FAIL, 1 N/D**, hardware media keys).
PR #13 is merged. v0.3 preserves this browser/session grant and adds protected
token persistence under [ADR-0018](adr/0018-persistent-session-and-settings.md);
browser cookies remain ephemeral. New persistence/backend/workspace checks and
owner acceptance are separate, recorded in [VALIDATION.md](VALIDATION.md).

The research below retains its **29 October** scope, source pins and concrete
navigation findings. Statements that the real chain was unverified or the token
was memory-only describe that earlier revision, not the current merged status
or v0.3 storage policy. Public service/registration distribution conditions and
the embedded-agent standards concern remain unresolved by technical acceptance.

Reviewed **2026-10-29** for PR #8, including the owner's subsequent scope
correction permitting adaptation of Cassette's published working auth chain
without requiring a Karin-owned registration. Research and automated validation
use public source and synthetic local fixtures only; no real account, session,
OAuth exchange or Music request was performed in Cloud/CI. Published shared
application values are attributed in the implementation, not repeated here;
user secrets and stream-signing material are never published. Source licensing,
provider permission, technical interoperability and live acceptance are separate.

## Fedora navigation regression after `77a1a27`

The owner confirms that this revision opens the real Yandex page on Fedora. Karin
then aborts with `UntrustedNavigation` after roughly five phone-input digits or
in the QR path. The concrete rejected event has not been captured; neither an
iframe nor a fragment nor a missing provider origin is established as the
observed Yandex cause. This failure is inside Karin's browser policy, not evidence
of Music API 403, wrong account credentials, VPN failure or denied service rights.

Source inspection nevertheless identifies defects worth correcting independently:
ordinary trusted URLs with a fragment are rejected by both URL/origin checks,
and an unknown-frame navigation rejection unconditionally terminates the entire
attempt. Those rules are inappropriate for an interactive provider page. Ordinary
navigation must remain separate from strict callback processing. OAuth-response
fragments are not ordinary page hashes, and callback detection must not consume
state or start token work until the main document is confirmed.

The WebKit UI API provides distinct evidence:

- [`NavigationAction.get_frame_name`](https://webkitgtk.org/reference/webkitgtk/stable/method.NavigationAction.get_frame_name.html)
  does not provide a reliable main-frame flag. An absent name is not main-frame
  proof. A rejected action with unknown context stays blocked, without treating
  that event alone as a fatal error for the whole session.
- [`WebView.get_uri`](https://webkitgtk.org/reference/webkitgtk/stable/method.WebView.get_uri.html)
  describes the main document, including provisional URI changes. It can establish
  main-document context; it does not turn an untrusted address into an approved
  page or an OAuth session into Music authorization.
- [`ResponsePolicyDecision.is_main_frame_main_resource`](https://webkitgtk.org/reference/webkitgtk/stable/method.ResponsePolicyDecision.is_main_frame_main_resource.html)
  returning true proves the main-frame main resource. False alone does **not**
  prove a subframe, because it may describe another resource of the main frame.
- The inspected WebKit [2.52.6](https://github.com/WebKit/WebKit/blob/webkitgtk-2.52.6/Source/WebKit/UIProcess/API/glib/WebKitNavigationClient.cpp#L64)
  and [2.54.0](https://github.com/WebKit/WebKit/blob/webkitgtk-2.54.0/Source/WebKit/UIProcess/API/glib/WebKitNavigationClient.cpp#L64)
  `NavigationClient.cpp` paths suppress subframe `load_failed` forwarding with an explicit `frameInfo.isMainFrame`
  guard. Cancelled loads caused by policy decisions or application stop must
  still be handled separately, without overwriting the earlier rejection/cause.

The finite approved origin list is retained. A new origin requires concrete
provider evidence; the reported failure does not justify wildcard domains,
substring matching, arbitrary internet navigation or disabling TLS/sandbox/state.
Service documents such as `about:blank` are not authenticated provider origins
and cannot complete authorization.

The first rejected event per attempt is reduced to a one-shot safe diagnostic:
fixed event, reason and confirmed main/subframe/unknown context, with canonical
scheme/host/port and fragment presence only for recognized public origins.
Malformed, credential-bearing, OAuth-result and unknown-host addresses produce
fixed redacted categories instead. No full address, path, query/fragment value,
phone/QR data, input fields, cookies, token or state is retained in that value.
It is not a full network log. See [the filtered Fedora procedure](BUILDING.md#one-fedora-navigation-retest-after-77a1a27)
and [revision-scoped validation](VALIDATION.md).

The interactive loopback test reproduces synthetic input/hash/frame behavior,
not Yandex's live phone or QR implementation. Actual token issuance, Music
account acceptance and personal library/search remain unverified until the
consenting local test. This correction does not change the Cassette-compatible
grant or route the user back to the refused Karin-owned registration.

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

The corrected prototype adapts the published registration and sessionid contract,
with upstream GPL notices retained. It does **not** adopt the substring check,
URL logging, persistent cookies/tokens, official-client Music header or
close-before-validation ordering. Public source/code licensing permits scoped
code adaptation under its terms; it does not itself grant rights to the OAuth
registration, Yandex service or content. Those conditions remain unresolved,
not an established blanket prohibition or a technical provider-unavailable error.

## Implemented Cassette-compatible provider

Default `cassette-auth` enables `auth-webkit`; the existing Flatpak prototype
selects it explicitly. The normal no-argument application uses this real provider,
not the known Karin-owned PKCE route or a permanently blocked provider.

1. `Войти` creates an ephemeral WebKitGTK6 NetworkSession, with persistent credential
   storage disabled, and loads the actual official Yandex authorize endpoint with
   `response_type=token`, Cassette's published client registration and fresh random
   state. No redirect registration or technical credential input is required from
   the end user. The consent screen may name that upstream registration rather
   than Karin's own OAuth app; the identity must not be misrepresented.
2. The provider checks the exact HTTPS `music.yandex.ru` root return and matching
   state, in query or fragment, with bounded/duplicate-safe parsing and one-time
   completion. Main-document completion is checked separately from frame navigation.
   The implicit token fragment is not logged, displayed or used as Music proof.
   Cassette relies on the registration's default redirect; the inspected source
   does not establish its actual returned path or echoed state. A missing state
   or another path therefore produces a concrete return-stage failure, pending
   the real Fedora run, rather than unsafe substring acceptance.
3. CookieManager asynchronously requests only cookies applicable to
   `https://oauth.yandex.ru/token` from **this voluntary Karin session**. It does
   not import other applications' cookies or read a database, password form, DOM
   or JavaScript result. A bounded redacted memory payload requires the relevant
   session cookie. Collection precedes clearing the ephemeral WebKit data.
4. A worker posts `grant_type=sessionid`, the same published client ID/secret
   and `host=oauth.yandex.ru` to official `/token`, with that scoped Cookie header.
   TLS checking, no redirects, timeouts and response limits remain enforced.
   Cookies are not forwarded to Music API. Token response/errors are reduced to
   safe typed values; no body, URL, personal field or secret reaches logs/UI.
5. The existing MusicService validates the account. Only success can close the
   dialog and independently start liked tracks and personal playlist loads;
   playlist contents, search, details and queue use the retained service/UI.
   Music denial remains a Music-validation error, not a successful Yandex login.

This is **not Authorization Code+PKCE**. The sessionid contract has no code
verifier; the separate own-client PKCE diagnostic is unchanged. Cancellation
aborts pending browser-cookie work, clears ephemeral data, invalidates the attempt
and discards late token/Music responses. Tokens/cookies remain process-memory-only;
no persistent credentials, refresh or remote revocation is implemented.

`src/auth/cassette.rs` adapts the GPL-3.0-or-later v0.2.4 implementation, preserving
Copyright 2023–2024 Vladimir Vaskov. The published shared app configuration is retained
separately in `src/auth/cassette_client.json`. The unmodified upstream GPL and
[provenance](../packaging/third-party-licenses/cassette-auth/PROVENANCE.md) are
preserved under `packaging/third-party-licenses/cassette-auth/`. This config is
shared published upstream application material, not a user's password/token;
its inclusion does not settle OAuth registration distribution conditions.

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

## Remaining facts, local test and distribution conditions

The owner now authorizes the scoped Cassette-compatible functional prototype;
a Karin-owned registration is no longer a precondition. The implementation
must attempt the real chain, not repeat the known own-client 403 or disguise a
local policy choice as a new provider response. The source contract is established;
**the new Karin chain's real consent/session/token/Music/library/search acceptance
is not yet established**. Use [the single Fedora procedure](BUILDING.md#music-sign-in-and-fedora-acceptance).

A failed run reports the concrete stage: browser/TLS/process, exact return/state,
missing/read-failed own-session cookie, `/token` HTTP and allowlisted OAuth error,
network/timeout/malformed/expired token response, Music account validation, or
library/search request. Only safe fixed categories/statuses may be shared. Do not
infer missing scopes, subscription, allowlisting, region or provider policy from
a generic failure; do not retry with another identity or region to conceal it.

Public distribution remains a separate unresolved question. The reviewed GPL
licenses establish code-reuse terms and notice/source obligations, **not** express
permission to use the registration or Music API as Karin. No provider confirmation
of Linux WebKit or those distribution conditions has been obtained. That is an
unknown permission/compatibility condition, not proof of a blanket service ban
or proof of approval. RFC 8252's embedded-agent objection remains applicable;
Yandex's documented mobile SDK WebView support does not resolve Linux eligibility.

The browser policy is not a firewall for every page subresource. The selected
WebKit navigation API alone cannot prove a main-frame return, so session completion
also checks the main document rather than trusting a subframe policy event.
The exact HTTPS origins `smartcaptcha.yandexcloud.net` and `captcha-api.yandex.ru`
are additionally permitted for Yandex-owned CAPTCHA navigation: the [official SmartCaptcha integration
documentation](https://yandex.cloud/en/docs/smartcaptcha/quickstart) and its
[published widget script](https://smartcaptcha.yandexcloud.net/captcha.js),
retrieved on 2026-10-29, establish this component's origin; the script explicitly
recognizes both exact hosts and builds its checkbox/challenge frames there.
This does not prove that a particular Yandex OAuth login invokes either host.
No wildcard Yandex or Yandex Cloud domain,
alternate port, HTTP origin or lookalike is allowed. It does not change the exact
Music completion or OAuth token destinations; `captcha.yandex.net` is not added
without equivalent affirmative evidence.
`--smoke-test` and the capture helper's explicit `--ui-fixture` provider are offline
only and do not exercise actual Yandex auth. Browser sandbox tests remain separate
from provider/session acceptance. Audio delivery retains its
[independent gate](STREAM_DELIVERY_REVIEW.md). See
[ADR-0011](adr/0011-cassette-session-auth-prototype.md) and
[API evidence](API_FEASIBILITY.md).

Ubuntu 24.04 CI additionally needs an AppArmor-labelled parent process to let
WebKit create its own sandbox. The workflow grants `userns` only to the exact
compiled test executable and removes that temporary profile after the test.
It preserves the global restriction and never disables WebKit's sandbox; this
is a test-runner prerequisite, not authorization to access Yandex services.
The model is Ubuntu's [Epiphany application profile](https://git.launchpad.net/ubuntu/+source/apparmor/plain/profiles/apparmor.d/epiphany?h=ubuntu/noble).

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
