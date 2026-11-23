<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0018: Protected sessions, preferences and private continuation

Status: Accepted; current persistence contract.

## Credentials and lifecycle

Keep token ownership, protected I/O, lifecycle, settings and GTK separate. Native uses
Secret Service with bounded unlock/prompt work. Flatpak uses the official Secret Portal
FD/request contract, HKDF-SHA256 and AES-256-GCM for an authenticated envelope in private
XDG data; its portal key stays in memory. No direct-keyring sandbox expansion or plaintext
fallback is permitted. Unavailable storage allows memory-only sign-in. Detection is not
proof of successful save/restore/delete.

Persist only a Music-validated access token, never passwords, browser cookies, sessionid,
diagnostics or media addresses. Restore off GTK, then revalidate Music before admitting
authenticated work. Network failure is not proof of revocation; a stored token is not
current authorization. Remote refresh/revocation is absent.

Serialize credential mutations and fence account/request generations. Record the
non-secret durable `forget_pending` barrier before forget/replacement work; only confirmed
completion may clear that reservation. A pending cache purge is a separate obligation that
a replacement token save cannot erase. Native ambiguous CreateItem writes retain a
non-secret pending marker: timeout plus an empty query does not prove no late commit.
Definitive refusals and verified absence remain distinct from uncertain deletion. Failures
retain visible Retry and honest durable suppression status; an in-memory flag cannot
promise restart protection.

Logout stops audio, clears personal state, purges cache and verifies credential absence.
Replacement login is blocked during active cleanup; settled failure may allow memory-only
use with restoration/persistence paused behind the barrier. Remember-off deletes durable
credentials/continuation while the live session may continue. Close may retain confirmed
permitted state. Admitted durable workers finish with application holds; replacement
windows wait asynchronously for retired transactions. Local forgetting does not revoke the
provider grant.

Non-secret schema-3 preferences use bounded atomic private regular files (0700
directories/0600 files) in the app's XDG config namespace. Read supported older schemas
without erasing unrelated settings/barriers; malformed recovery must not revive a saved
login. Credentials and cache occupy separate namespaces. Only fixed non-secret transaction
state is written outside protected storage; Rust-owned secret buffers use zeroizing
wrappers without claiming universal OS/ GLib/HTTP-copy erasure.

## Ordinary playback continuation

Store at most 500 account-bound selectors and source positions, current index/ position,
manual entries, Shuffle/history/RNG and Repeat under private XDG data. Atomic checksummed
records contain no display metadata, tokens, URLs, XML, cookies, Wave IDs or occurrence
tokens. Restore creates fresh opaque occurrence identities. Debounced/event and periodic
saves run on ordered application-held workers; tombstones/generations/barriers fence late
close/save results. An empty queue before restoration must not overwrite retained state,
and failed lookup suspends snapshot saving until deliberate Retry rather than publishing a
null record.

Fresh Music validation and canonical 50-selector batches precede restoration; prune
missing/unavailable positions without merging duplicates or losing history. Bound saved
position by current duration. Restore makes no collection/resolver/ media/Wave request,
prepares no player and does not autoplay. Explicit Play uses fresh source admission;
nonseekable restoration explains starting at zero. Stop tombstones continuation while
retaining the in-memory queue; close cannot revive it until new Play. Logout/forget delete
it. Successful Wave replacement deletes ordinary continuation; radio itself never
persists. Cache Clear is independent. Saved provenance is not current server-playlist
membership.

Actual keyring/portal availability has its own desktop scope; fixtures are not platform
acceptance. See [BUILDING](../BUILDING.md), [LIMITATIONS](../LIMITATIONS.md) and [Secret
Service](https://specifications.freedesktop.org/secret-service-spec/latest/).
