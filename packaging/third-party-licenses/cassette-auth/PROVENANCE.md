<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Cassette authentication adaptation

Karin's Cassette-compatible authentication provider is a Rust adaptation of the
publicly published session-based flow in Cassette v0.2.4:

- Project: https://github.com/Rirusha/Cassette
- Immutable revision: `fc6c6e689bd0c60fa662b03aa2aa3c05534b4811`.
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
