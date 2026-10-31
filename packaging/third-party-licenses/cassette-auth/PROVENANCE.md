<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Cassette authentication adaptation and legacy playback reference

Karin's Cassette-compatible authentication provider is a Rust adaptation of the
publicly published session-based flow in Cassette v0.2.4:

- Project: https://github.com/Rirusha/Cassette
- Immutable revision: `fc6c6e689bd0c60fa662b03aa2aa3c05534b4811`.
- Source commit behind that annotated tag:
  `9d34251d01bcd414ffbec2bef7931090d33c9124`.
- Login: `src/widgets/begin/begin-view.vala`, lines 47–104 and 125–135.
- Token exchange: `src/client/api/yam-client.vala`, lines 53–89.
- Cookie transport: `src/client/soup-wrapper.vala`, lines 107–151.
- Original copyright: 2023–2024 Vladimir Vaskov.
- Original SPDX license: GPL-3.0-or-later. The accompanying `COPYING` is the
  complete verbatim upstream GPLv3 license text.

`src/auth/cassette_client.json` contains that revision's already public shared
application configuration. These values identify the upstream OAuth registration;
they are not a Karin-owned registration, private user token or user password.
Their presence is disclosed rather than disguised as Karin-owned credentials.
Karin does not send Cassette's official Android-client identity headers.

The adaptation keeps cookies only in an isolated, ephemeral WebKit session and
memory during its exchange; it does not import another application's sessions or
copy Cassette's SQLite persistence, broad substring return check, URL logging or
close-before-Music-validation behavior. It adds state validation, bounded HTTP,
explicit cancellation and account validation through Karin's existing MusicService.

GPL permits adaptation of the published software under its terms. It does not
itself grant rights to the upstream OAuth registration, prove Yandex's approval
of this Linux embedded flow or establish public-distribution permission. Those
questions are separate from technical token issuance and local API acceptance.

## Legacy playback protocol reference

The guarded original Rust `stream::StreamResolver` also compares Cassette
v0.2.4's `yam-client.vala` offer/XML/checksum resource flow (lines 1221–1280)
and `player.vala` playbin/EOS handoff (lines 492–520), using the source commit
above. Existing copyright attribution and the verbatim upstream `COPYING` remain
packaged. No Cassette audio, artwork, cache or UI resource is copied.

The legacy protocol contract was independently compared with MarshalX
`9ca132e5c18a72aab779c1fc68c169380a9357d0`, yamuse
`368c74646520e8bcd30c44a9887e1bf99cb58172` and lifeart/ya-namp
`49cfee7002ebf816f4144860e67d6fa3be5f5708`. This is not adaptation of yamu's
modern extracted official-client signer or protected-media/decryption path.
Protocol/source references and GPL notices do not establish a grant to the
shared registration, permission to redistribute service music or public release
approval. The consenting local prototype's technical results are recorded
separately from those unresolved service/distribution conditions.
