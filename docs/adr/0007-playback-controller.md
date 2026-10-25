<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0007: Independent playback through a bounded GStreamer worker

Status: Accepted
Date: 2026-10-25

## Context

Gate A needs a real player engine even while Karin-specific Yandex Music
authorization and permitted stream delivery remain blocked. The v0.0 finite tone
diagnostic proves a narrow audio path but cannot provide interactive transport,
seeking or a retained playback lifecycle. GTK must remain responsive during
preroll, errors, rapid commands and shutdown.

## Decision

Keep one Cargo package and the approved GStreamer stack. Add a dedicated
`playback` module whose worker owns an audio-only `playbin`. GTK/application
callers send prepare, play, pause/resume, stop, volume and seek commands through
a bounded 32-entry nonblocking queue and observe typed snapshots. The worker
continues observing bus errors/EOS while processing bounded command batches and
enforces a five-second deadline for asynchronous state completion. The deadline
does not preempt a hung native plugin call; trusted inputs and worker isolation
remain important even though GTK does not wait for those calls.

Expose Idle, Loading, Ready, Playing, Paused, Stopped, Ended and Error separately.
Reject unsupported operations visibly; allow seeking only when reported by the
prepared source. Keep displayable errors and debug values free of source paths,
raw GStreamer diagnostic strings and future credential-bearing URLs.

Accept regular local files in Gate A. Do not add a remote-URI source or implement
the future permitted-stream boundary until the authorization, entitlement and
service-policy prerequisites in [ADR-0006](0006-music-authorization-feasibility.md)
are satisfied. Generic Yandex ID OAuth is not a Music grant.

Exercise the actual local-file playback path using a generated quiet PCM WAV.
The developer diagnostics section and `--playback-self-test` identify this source
explicitly. Temporary files are securely created and owned by a cleanup guard;
no copyrighted recording, fake catalog or packaged audio fixture is introduced.
Use a synchronized `fakesink` for clocked, device-independent tests. Request
system audio only through an explicit developer choice.

Keep shutdown independent of queue capacity. Cancellation is signalled
immediately; the worker resets and drops the pipeline before publishing its
completion. Last-handle drop must not join a worker on GTK's main thread. A caller
requiring verified cleanup must observe completion with a bounded deadline;
forced process termination still cannot provide that guarantee.

## Consequences

The engine can be tested without network, accounts, service music or an audio
device, while using real GStreamer decoding and transport rather than simulated
success. The finite v0.0 audio diagnostic remains available independently.
State, media loading, seek support, EOS/error recovery, rapid commands and
pipeline cleanup require direct tests; successful compilation is insufficient.
Executed results belong in [VALIDATION.md](../VALIDATION.md), not this decision.

The generated WAV path establishes neither support for every codec nor a
permitted Yandex Music stream. Physical output, PipeWire behavior, the new GTK
controls and the exact Flatpak artifact need fresh Fedora acceptance. GNOME 50,
the Freedesktop 25.08 Rust extension, Rust 1.98.1 compatibility and current
Flatpak permissions are preserved.
