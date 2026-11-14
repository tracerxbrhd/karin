<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0033: Independent UI style and color scheme

Status: Accepted; PR #28 normally merged at `2ab6d28` after owner v0.7
Native/installed-Flatpak acceptance at `949d1f9`. External Karin design remains unavailable.
Date: 2026-11-11
Extends: ADR-0013, ADR-0018 and ADR-0030

## Context

Karin already has a working GTK4/Libadwaita workspace and persistent
`Appearance::{System,Light,Dark}` preference. These values describe color
selection, not two different interface designs. The owner's new direction calls
for the current native System presentation and a separate future Karin design.
The designer's materials have not been delivered; copying System under another
name or inventing a branded design would misrepresent implementation readiness.

## Decision

Retain the existing enum and `appearance` wire field, with `ColorScheme` as a
compatible typed alias. Add independent `UiStyle::{System,Karin}`. Both choices
default to System. Preferences schema 3 reads schemas 1 and 2 without resetting
remembered-login/deletion intent, the aggregate cache policy or the pinned pane.
Malformed/unrecognized style values alone fall back safely to System without
discarding those other preferences. Existing atomic settings writes stay in
use; no credentials, queue state or media are moved into this settings file.

Only System is available. Preferences shows Karin as disabled, «В разработке».
A recognized persisted Karin request is retained, rendered with an explicit
System fallback and can be replaced by selecting System. It does not activate
a second name for the same finished theme.

Keep presentation application in `ui::style`: attach a presentation root, load
the scoped System CSS from `resources/styles/system.css` and apply color through
`AdwStyleManager`. The comment-only `resources/styles/karin.css` reserves a
separate `.karin-style-custom` integration boundary and is not loaded. Existing
System selectors use `.karin-style-system`. Future replacement views must share
the current view models, typed callbacks, MusicService, queue, PlaybackController,
cache, Wave and MPRIS; a style change cannot create or restart those components.

System scheme delegates to Adwaita's supported settings source; manual choices
use ForceLight/ForceDark and returning to System releases the override. Retain
semantic palette/accent roles, inherited fonts and scale, native focus/accessibility
and GTK reduced-motion preferences. A live high-contrast class removes custom
translucent metadata and outlines the current row. Do not set
`gtk-application-prefer-dark-theme`, raise the library API floor for accent support,
or promise complete Qt/Breeze CSS inheritance.

## Consequences and verification

The System interface remains the product UI. The future branded presentation
requires actual licensed designer materials, both schemes, responsive/loading/
error/empty states and accessibility validation before availability is enabled.
The handoff contract is recorded in
[KARIN_STYLE_INTEGRATION](../design/KARIN_STYLE_INTEGRATION.md).

Migration tests cover old schemas/colors, independent persistence and preservation
of deletion barriers/cache choices. Native tests exercise synthetic Adwaita
system-light/dark/high-contrast sources, manual overrides, return to System,
font/reduced-motion inheritance and honest unavailable-style controls. Application
integration tests verify that presentation changes preserve the active player,
queue, cache and MPRIS. These fixtures do not certify live KDE/GNOME portals or a
designer theme that does not yet exist; desktop Native/Flatpak acceptance remains
separate from Cloud results.
