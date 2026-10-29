<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0012: Independent catalog requests and an owner-local delivery probe

Status: Accepted for Issue #9 implementation; live catalog/audio acceptance pending
Date: 2026-10-29
Supersedes: ADR-0009's deferral of all delivery-info investigation

## Context

The owner reports that the PR #8 prototype at `797ba3a` signs in through Yandex,
validates the Music account and loads liked tracks and personal playlist names
on Fedora 44. Search and opening those playlists fail. These facts establish
neither the failing request/category nor a service audio resource. The original
own-client PKCE 403 is a separate experiment, not the current login failure.

[Issue #9](https://github.com/tracerxbrhd/karin/issues/9) makes audible Music
playback mandatory for v0.2 and requires investigation in parallel with these
catalog repairs. Preserve the working Cassette-compatible login, MusicService
and existing GStreamer engine. Do not merge the stacked branches automatically.

## Decision

Replace the single catalog admission lock with bounded independent request
lanes. Session and per-operation generations prevent superseded results from
altering the new view or session. Navigating to a detail starts a visible loading
page immediately; missing data, an empty collection and a failed request are
different states. Preserve identity validation rather than weakening it to
accommodate a response variant. Compare bounded wire models and paging to pinned
upstream contracts and exercise unexpected but valid variants with local fixtures.

Add an explicit `--delivery-diagnostic` local launch mode. It uses the normal
login in that window and a selected real track; no credentials, IDs or URLs are
command-line inputs. Only after a validated Music session may the owner request
bounded read-only delivery information. This mode does not request media,
construct signatures, retrieve keys or cache audio. A separately allowlisted
metadata-only step may classify the legacy negotiation. It returns fixed
categories and HTTP status in a native inline component. Raw responses, track
IDs, resource URLs, cookies and tokens never become reports or logs. Automated
tests use loopback fixtures and synthetic session values.

The legacy per-track delivery-info step is distinct from modern negotiation
requiring extracted official-client signing material. A server-returned signed
resource is not automatically DRM; a legacy `direct` flag or codec alone is not
a complete validated playback source either. Investigate the actual next step
before accepting a source. Do not port extracted signing/decryption constants or
change the current experimental provider's identity to evade a refusal.

An eligible source must eventually pass a separate resolver boundary with
authorization, HTTPS origin/redirect/expiry and credential-leak controls before
GStreamer receives it. Do not provide an arbitrary URI entry point or turn a
generated WAV into a selected service track. Real playback controls and MPRIS
may report playing only after the actual media engine does so.

## Consequences

The first repair increment contains executable catalog fixes and an executable
local audio investigation, not another source-only deferral. Independent native
navigation, artwork and supported interactions can progress if delivery is still
blocked. The exact probe outcome must determine the next audio implementation.
Until an entitled owner hears an actual selected track through PipeWire/GStreamer
on Fedora, the overall v0.2 audible-player criterion remains **not met**.

The prototype's source license, unresolved registration/service distribution
conditions and concrete HTTP/schema errors remain separate. No persistent
session, service permission, live catalog fix or audible acceptance is inferred
from fixtures, a build, a screenshot or a successful account response.
