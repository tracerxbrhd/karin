# ADR-0002: One package, service boundaries and GIO workers

Status: Accepted
Date: 2026-10-24

## Context

The first milestone needs a real native window, a safe network boundary and an
independently testable audio engine. The unofficial service's authenticated
capabilities remain unverified. GTK's main loop must stay responsive.

## Decision

Use one Cargo package with internal domain, application, API, audio, platform,
configuration and UI modules. Inject a `MusicApi` implementation into the
application service; UI consumes domain values only.

Use a bounded reqwest blocking adapter and GIO workers. Return results through
main-context futures and keep GTK objects on the main thread. Retain an application
hold until each bounded job has completed so audio cleanup precedes normal exit.

Use programmatic gtk-rs/Libadwaita widgets for the small initial shell. Blueprint
would add resource-generation steps without reducing this shell's complexity;
reconsider it when reusable UI templates exist. This does not replace the approved
native stack.

Keep synthetic audio separate from music service access, and do not export MPRIS
or read Secret Service items before those integrations are implemented.

## Consequences

Transport and service errors can be tested offline. The GUI remains interactive
during diagnostics without an additional top-level Tokio runtime. Normal closing
can wait for an outstanding bounded operation; immediate cancellation of future
long-lived playback requires a dedicated lifecycle design. Multiple crates can
be introduced if component reuse or compile boundaries later justify them.
