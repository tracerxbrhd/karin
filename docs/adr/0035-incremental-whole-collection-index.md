<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0035: Bounded catalog search and collection indexing

Status: Accepted; current search/provenance contract.

## Typed search and paging

MusicService fixes HTTPS `api.music.yandex.net`, five-second connect/fifteen-second normal
request limits, 1 MiB bodies, no redirects/retries and strict account/ID binding. Only
allowlisted error codes refine fixed categories; generic403 proves no particular missing
scope, subscription, region or identity cause.

Track search reads minimal validated own IDs and ignores optional display shapes, then
normalizes at most 50 raw positions/50 distinct selectors through one canonical `/tracks`
batch. No N+1 or account probe per row. Preserve provider order, composite selectors and
duplicates; null/missing records count as omissions and unavailable normalized tracks stay
disabled. Strict identity/envelope/paging failures remain errors. Provider totals/pages,
not post-lookup count, govern continuation. Independent unified sections retain successful
categories when another fails. One plain-language error/Retry replaces duplicate technical
banners.

Ordinary views admit 50-position portions, at most 2,000 positions/100 pages and three
initial viewport-fill pages. Actual forward demand admits later work; sparse pages do not
trigger unlimited draining. Preserve rows/scroll and original source positions on append.
Errors keep prior data and require explicit Retry. Refresh/ query/source/account changes
retire old tickets; applied page identity prevents replay duplication. Membership-changing
writes stop old liked-feed continuation until explicit refresh, without modifying active
playback.

## Deliberate complete supported-collection path

Loaded metadata filters immediately with precomputed Unicode normalized/casefolded
title/artist/album keys. A settled nonempty query/Enter may start or resume an eligible
liked/playlist index; opening/navigating alone never scans the library. Read a compact
manifest bounded at 4 MiB/20,000 original positions with strict owner/kind/revision/count.
Normalize consecutive 50-position slices, preserving holes and duplicates, then recheck
the complete membership sequence/revision before declaring complete. Unsupported
larger/changed/incomplete coverage stays explicit; an unfinished prefix cannot prove no
remote match.

Keep one process-only reusable account-bound index, latest-query projection and bounded
200-row windows. Query changes do not restart metadata reads. Clear/leave suspends work;
return alone cannot resume it; deliberate Continue/Retry does. Sorting uses maintained
keys and stable original-position ties. Added-date order requires every timestamp valid;
missing/unsupported dates disable it without rejecting tracks or guessing dates. Total
duration requires complete known data. There is no persistent index/database, startup scan
or per-keystroke HTTP.

Explicit Play snapshots an aligned window of at most 500 selected-projection occurrences,
keeping original positions below 20,000; cache at most four windows.
Filtering/sorting/appending/server edits never rewrite the active context. Manual items
retain separate provenance; current markers/Info/MPRIS use opaque queue occurrences, not
mutable indices or equal IDs.

## Native search intent and verification

The window mounts the three section buttons in the native HeaderBar's title slot.
At compact widths the buttons retain their accessible names and show icons alone.
One global entry shares the same measured title slot. The leading search button,
Ctrl+F and Home focus it without route change until nonempty results; 350 ms settled
input or Enter submits. Clear restores source with field open; Escape/toggle/background
dismiss and retire work while preserving interactive targets. The collection entry fills
its content column beside the compact sort button. [BUILDING](../BUILDING.md) retains
actual input, bounded-request, stale-result and queue-preservation regressions.
