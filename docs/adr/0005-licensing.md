<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# ADR-0005: GPL-3.0-or-later for Karin

Status: Accepted
Date: 2026-10-24

## Context

The project owner has approved GNU GPL-3.0-or-later as Karin's license. The initial
foundation left this decision open. Karin links native libraries and includes
Rust dependencies with their own licenses; choosing an application license does
not replace those licenses or authorize access to a remote music service.

## Decision

License Karin's own code and original materials under GNU GPL version 3 or, at
the recipient's option, any later version, unless an individual material says
otherwise. Use the SPDX identifier `GPL-3.0-or-later` in Cargo metadata and source
headers, and the corresponding GPL-3.0-or-later license type in the About dialog.
`LICENSE` contains the full, unmodified standard GPLv3 text. Its text must not be
edited or replaced with a project-specific paraphrase.

The choice keeps distributed modifications and combined derivative work under
copyleft terms, provides recipients the GPL's source and modification rights,
and permits compatible future GPL versions without making the application
proprietary. The GPL applies to distribution of the covered work; it does not
require publishing every private modification merely because it was made.

Third-party source, notices, artwork, dependencies and runtime components retain
their own licenses. Preserve supplied license and copyright notices. The locked
Rust dependency declarations and relevant native components are reviewed in
[LICENSING.md](../LICENSING.md); dependency changes require a renewed review.
Flatpak source preparation keeps the original vendored source and gathers supplied
third-party notices for installation alongside Karin's license.

## Consequences and limits

Distributors must meet the GPL's applicable corresponding-source and notice
requirements and any compatible dependency obligations; a binary and a license
file alone are insufficient. Native/runtime and codec selection require review
for the exact artifact being shipped. The inventory is engineering evidence,
not a complete legal opinion or a release certification.

GPL licensing grants rights only in covered software/materials. It grants no
Yandex trademarks, account rights, audio rights, API permissions or exemption
from Yandex Music's terms. Authentication and playback restrictions in
[API_FEASIBILITY.md](../API_FEASIBILITY.md) remain in force.
