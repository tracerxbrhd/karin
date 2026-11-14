<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0032: Confirmed, occurrence-aware playlist management

Status: implemented; live Fedora acceptance pending.

## Context

Playlist creation already uses MusicService and independent confirmation.
Editing a loaded prefix or using a track ID as its occurrence would remove or
move the wrong duplicate. Unofficial metadata endpoints also do not share one
documented revision compare-and-swap contract.

## Decision

Keep transport in `music::playlist_management` and native dialogs in
`ui::music::playlist_management`. Reuse the current account, HTTP client,
session cancellation and shared library-write admission; introduce no second
service or player. The service serializes playlist writes across its clones,
including Create. GTK submits one explicit intent at a time, through a dedicated
request lane independent of navigation reads.

An editable snapshot requires current validated ownership, the exact owner/kind,
an explicitly supplied unsigned revision, and the complete server sequence of
at most 2,000 positions. Missing revision is not zero. Each occurrence retains
its server position and checked track/album selector, including unavailable
metadata and repeated IDs. Unsupported optional rich display fields cannot
compact that sequence. The editor pages this snapshot in groups of 50; a
partially loaded catalog view is never a mutation snapshot.

Immediately before writing, read again and compare revision, sequence, title
and known visibility. Insert/Remove/Move use one revision-bound `/change` POST;
Move is one delete+insert diff batch, not two writes. Rename, Visibility and
Delete use their published separate endpoints. They have no confirmed server
revision CAS: fresh preflight plus readback detects observed conflicts but
cannot promise atomicity against an external writer after preflight.

Every intent sends at most one POST. Require a recognized acknowledgment and
independent fresh GET confirmation of the intended state. A lost/malformed
acknowledgment remains Unknown even if a later GET resembles the intent; only
GET reconciliation is offered, never automatic write replay. Revision/sequence
changes produce Conflict. Deletion needs explicit `"ok"`/`true`, detail absence
through HTTP 404/410, and absence from a successful own-list read. Missing
visibility is Unknown, not Private. Public/private changes are disabled until
the state is established. Destructive deletion names the playlist and uses an
Adwaita confirmation with Cancel as default/close response.

Playlist changes refresh matching surviving catalog views and invalidate their
cache without rewriting the active queue, stable occurrences or audio. Closing
a dialog retires presentation, not an already accepted remote request. Account,
view and weak-widget fences reject late presentation updates.

## Evidence and limitations

Contracts were compared with MarshalX/yandex-music-api
`9ca132e5c18a72aab779c1fc68c169380a9357d0`, Cassette v0.2.4
`9d34251d01bcd414ffbec2bef7931090d33c9124`, and their referenced public OpenAPI.
Cassette's unfinished `change-relative` method is not used. See
[the contract table](../PLAYLIST_MANAGEMENT.md). The implementation is original;
existing upstream attribution and GPL notices remain intact.

Local HTTP and ordinary-window tests cover contracts and races without a live
account. Executed results belong in [VALIDATION](../VALIDATION.md); these tests
do not establish provider acceptance or public service/content distribution
rights. The owner's one disposable-playlist Native/Flatpak acceptance remains
required before claiming real-service completion of issue #19.
