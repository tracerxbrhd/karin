# ADR-0001: Native Linux Application Stack

Status: Accepted
Date: 2026-10-24

## Context
Karin requires a responsive, maintainable native Linux UI
with audio playback and desktop integration.

## Decision
Use Rust, GTK4, Libadwaita, and GStreamer.
Keep Yandex Music integration behind a dedicated API boundary.
Target Flatpak as the primary distribution format.

## Consequences
The application remains native and Linux-focused.
GTK/GStreamer development dependencies are required.
API availability, authentication, and playback feasibility
remain separate validation requirements.
