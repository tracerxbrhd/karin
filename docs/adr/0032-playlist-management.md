<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0032: Confirmed library reactions and playlist management

Status: Accepted; current mutation contract.

## Service admission and confirmation

Keep transport in existing MusicService interaction/playlist modules and native dialogs in
GTK. A validated account determines every UID/owner/kind path. Serialize writes across
service clones and admit one explicit intent at a time, independent of navigation reads.
One intent sends at most one POST; require recognized ACK and independent fresh readback.
Lost/malformed/ambiguous ACK remains Unknown; recovery is GET-only, never write replay or
a compensating mutation. Account, request, occurrence and weak-view fences prevent late
effects in replaced state. Invalidate relevant cache after every outcome, not only
success.

Like and Dislike are distinct memberships; removing either does not create the other. Read
strict compact UID/revision/selector state. Dual-read mutations must confirm
requested/opposite-mark state and non-regressing target revision. If Dislike is
unavailable, an independent current Like read preserves the existing Like-only contract,
leaving Dislike unknown/hidden. Do not infer the complement or let cached metadata confirm
a reaction. Only dual-read-confirmed Dislike addition on the unchanged current occurrence
invokes Next once; Like, removal, uncertainty and later GET recovery cannot replay that
transport effect. Wave uses its existing heard-track Skip lifecycle, without new
negative-feedback events.

## Exact playlist edits

A `richTracks=true` detail snapshot requires fresh validated ownership, explicit unsigned revision and
complete exact sequence of at most 2,000 positions matching trackCount. Keep unavailable
entries and duplicate positions; absent revision is not zero. The editor displays
50-position pages; a loaded catalog prefix is not an edit snapshot. Before POST compare
fresh revision, sequence, title and known visibility.

| Intent | POST suffix / confirmation |
| --- | --- |
| Insert/remove/move | `/change` with kind/revision/one diff batch; advancing revision and exact sequence readback |
| Rename | `/name`, `value`; matching ACK and exact intended state readback |
| Visibility | `/visibility`, public/private `value`; independently confirmed known state |
| Delete | `/delete`; explicit `"ok"`/true, detail HTTP404/410 and successful own-list absence |

Move is one delete+insert batch at the final post-removal position. Never guess an
insertion/move album selector; removal targets the exact known server position. Rename is
trimmed nonempty Unicode, at most 200 characters/1,024 UTF-8 bytes and no controls.
Unknown visibility disables change. Metadata/delete endpoints have no proven revision CAS:
preflight/readback detects observed conflict but cannot exclude another client writing in
between. Do not invent atomicity or rollback. Delete names the target and uses native
confirmation with Cancel default/close. Edits refresh surviving views without changing
active queue, playback or saved provenance. Closing a dialog cannot unsend an accepted
remote request.

## Evidence and verification

[Pinned
MarshalX](https://github.com/MarshalX/yandex-music-api/tree/9ca132e5c18a72aab779c1fc68c169380a9357d0)
and
[Cassette](https://github.com/Rirusha/Cassette/tree/9d34251d01bcd414ffbec2bef7931090d33c9124)
are protocol references, not API guarantees; unfinished `change-relative` is not used.
[LICENSING](../LICENSING.md) separates source attribution from service permission.
