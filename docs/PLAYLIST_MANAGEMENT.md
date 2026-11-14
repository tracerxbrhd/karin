<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Playlist management contracts

Implemented in `MusicService::playlist_management`; the GTK editor/picker calls
that service and does not build URLs, diffs or optimistic server state.
[ADR-0032](adr/0032-playlist-management.md) records the decision.

The references are MarshalX/yandex-music-api
[`9ca132e`](https://github.com/MarshalX/yandex-music-api/tree/9ca132e5c18a72aab779c1fc68c169380a9357d0)
(`yandex_music/client.py`, `_client/playlists.py`, `utils/difference.py`) and
Cassette v0.2.4
[`9d34251`](https://github.com/Rirusha/Cassette/tree/9d34251d01bcd414ffbec2bef7931090d33c9124)
(`src/client/api/yam-client.vala`), compared with public OpenAPI descriptions.
These are unofficial technical contracts, not an API stability or distribution
permission guarantee. No third-party OAuth identity or protected-delivery code
is added for playlist management.

All paths below are relative to the existing exact Music API origin and the
validated account's `/users/{uid}/playlists/{kind}`. Identifiers are checked;
tokens remain in the existing authorization header and never in paths/logs.

| Action | Request | Confirmation |
| --- | --- | --- |
| Read editor | GET detail with `richTracks=true` | Exact owner/kind, explicit revision, full sequence matching `trackCount` |
| Add track | POST `/change`, form `kind`, `revision`, JSON `diff` with `insert` and checked track/album | Recognized playlist ACK, advancing revision, independent exact sequence readback |
| Remove occurrence | Same route, `delete` range `[position, position + 1)` | Same revision/sequence checks; repeated IDs are separate positions |
| Move occurrence | Same route, one diff batch: delete old position, insert at final post-removal position | One POST/one revision-bound change, independent exact sequence readback |
| Rename | POST `/name`, form `value` | Matching playlist ACK and independently read exact intended title/state |
| Public/private | POST `/visibility`, form `value=public` or `private` | Matching ACK and independently established requested visibility/state |
| Delete | POST `/delete` | Explicit `"ok"`/`true`; detail GET 404/410 and successful own-list absence |

Rename accepts a trimmed nonempty Unicode title up to 200 characters/1,024
UTF-8 bytes, with no control characters. Unknown visibility stays unknown and
cannot be changed until refreshed to an established value. An entry without
the album selector needed for insertion/move cannot be moved by guessing it;
removal still targets the exact known server position.

No metadata/delete endpoint has a confirmed server-side revision CAS. All
changes first compare a fresh complete snapshot; this prevents knowingly stale
writes but cannot exclude another client writing between preflight and POST.
Independent readback detects observed mismatches; no rollback is invented.

## Native workflow

- Track actions, the track information pane, queue items and the current-track
  context menu offer “Добавить в плейлист”. The native picker reads a coherent
  owned list and validates the selected playlist before insertion.
- An owned playlist's detail header offers “Управление плейлистом”. Its complete
  server snapshot is bounded at 2,000 positions and displayed 50 rows per page.
  Rename, known visibility, per-occurrence Up/Down/Remove and deletion use that
  snapshot. Foreign playlists remain read-only.
- Loading/errors remain inside native dialogs. Conflict/Unknown offer Refresh
  using GET only. A lost ACK never becomes a silent retry or guessed success.
  Delete names the target and requires explicit confirmation; Cancel is default.
- Edits leave the playing GStreamer pipeline, active queue/duplicates, Wave
  lifecycle and MPRIS transport unchanged. Refreshed library details do not
  rewrite a previously selected playback context. Saved continuation describes
  its own queue, even if the original playlist is renamed or deleted.

## Verification boundary

The authored loopback tests cover all six mutation types, duplicates/composite
IDs, missing revisions, malformed optional metadata, both move directions,
one-POST behavior, lost/negative ACK, readback mismatch, concurrent admission,
account retirement, ownership and bounded data. Ordinary-window integration
also exercises playlist edits alongside the existing player. Consult
[VALIDATION](VALIDATION.md) for what actually executed and
[one final Fedora procedure](BUILDING.md#v07-final-fedora-acceptance) for the
remaining Native/installed-Flatpak check against a disposable personal playlist.
