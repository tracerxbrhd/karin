# ADR-0003: Anonymous service diagnostics before account integration

Status: Accepted
Date: 2026-10-24

## Context

Karin needs a real UI-to-service-to-adapter-to-domain path without pretending that
authentication or music playback has been proven. The anonymous account/status
endpoint returns a usable status envelope in the cloud environment, while search
and selected metadata requests return HTTP 451. A supported OAuth grant and Music
permissions for Karin's own client have not been validated. Community client
credentials cannot supply that authorization.

## Decision

Keep a narrow, anonymous, read-only status probe behind the API adapter and
application-service boundary. Map its result into typed domain state, preserving
service availability separately from authentication. In particular, HTTP 200
with `serviceAvailable=false` or no account identity is not a logged-in or playable
state. Display explicit loading, unauthorized/unavailable and error states.

Do not implement sign-in, personal library operations, stream resolution or My
Wave until the supported authorization, service terms and entitlement questions
in [API feasibility](../API_FEASIBILITY.md) are resolved. Do not store credentials
or add a token-input shortcut. Future storage belongs behind Secret Service.

Keep synthetic audio diagnostics independent of the API path and label them as
tests. Automated tests use local responses and fakesink; live service access is
an explicit manual check and is never required by CI.

## Consequences

The technical foundation has a genuine network slice and a genuine audio slice
without claiming an end-to-end music player. Errors and restrictions remain
visible. Extending the adapter requires new evidence, typed contracts and tests;
successful compilation or a community implementation alone is insufficient.
