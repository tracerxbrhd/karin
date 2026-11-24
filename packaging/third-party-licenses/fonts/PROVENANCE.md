<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Bundled typefaces

Unmodified upright variable fonts; no italic files or font installation. Both
include Latin and extended Cyrillic. Karin uses weights 400, 500, 600 and 700.
The original OFL 1.1 texts and copyright notices are retained beside this file.

| Resource | Official source and revision | SHA-256 |
| --- | --- | --- |
| `resources/fonts/Montserrat-VF.ttf`, Montserrat 9.000 | [Google Fonts](https://github.com/google/fonts/tree/26975696d3c3e450ebae6be2da3f2b86ad65500e/ofl/montserrat), `Montserrat[wght].ttf` | `0f7b311b2f3279e4eef9b2f968bcdbab6e28f4daeb1f049f4f278a902bcd82f7` |
| `resources/fonts/InterVariable.ttf`, Inter 4.1 | [Inter upstream](https://github.com/rsms/inter/tree/e3a3d4c57d5ecc01453a575621882a384c1995a3/docs/font-files), `InterVariable.ttf` | `4989b125924991b90d05b2d16e0e388c48f7d5bb8b30539bbf9c755278d0ccaf` |

The exact Fontconfig families are `Montserrat` and `Inter Variable`. The files
are embedded in the executable and registered with process-local Fontconfig
from sealed anonymous descriptors. Native and Flatpak need neither a network
request nor an operating-system installation of these typefaces. Application
CSS opts in to the families; OAuth HTML and platform window controls keep their
own presentation.
