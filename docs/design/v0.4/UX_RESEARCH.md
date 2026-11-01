<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Complete Music Experience — observed references and design decisions

Research date: **2026-11-01**. This is source-grounded UI research, not a claim
that either external application was installed or audited interactively. Public
images were fetched and opened locally, not inferred from help text. They remain
outside the repository: no external music art, personal screenshot or reference
code was copied into Karin. The resulting SVGs use Karin's own abstract artwork.

## Sources actually inspected

| Source | Evidence / scope |
| --- | --- |
| [Yandex Music desktop landing](https://music.yandex.ru/download) | Retrieved page and its current `app.js` build `1787926764`; opened [Windows asset](https://music.yandex.ru/download/images/promo-win.png?v=7329dc58) (2348×1216) and [macOS asset](https://music.yandex.ru/download/images/promo-mac.png?v=c82443d8) (2120×1318). These are currently published marketing images, not a live authenticated desktop run. |
| [Official player functions](https://yandex.ru/support/music/ru/listening-and-settings/player-functions) | Retrieved current HTML and Markdown, revision `e2011dcd592e77c694bebe7dceb186fac5507044`. Queue and playback-modes sections explicitly describe upcoming/history, Repeat once for playlist/twice for track, and Shuffle. Help documents behavior, not exact current desktop geometry. |
| [Cassette source](https://altlinux.space/rirusha/Cassette) | Queried recursive tree at current main `61eb9c662f7a59e9f5afe059641c4bcfbe32d05f`; opened its published [liked image](https://altlinux.space/rirusha/Cassette/raw/commit/61eb9c662f7a59e9f5afe059641c4bcfbe32d05f/app/data/images/1-liked-view.png) (847×894) and [queue image](https://altlinux.space/rirusha/Cassette/raw/commit/61eb9c662f7a59e9f5afe059641c4bcfbe32d05f/app/data/images/4-playback-queue.png) (999×892). |
| [Cassette Flathub](https://flathub.org/apps/space.rirusha.Cassette) | Current listing advertises **0.2.4**; its six-image carousel includes liked view, playlists, lyrics, queue, waves and preferences. Opened its actual liked image. These remain release screenshots; availability on today's page does not make them a new runtime version. |
| Cassette source qualification | Current main is a rewrite: pinned `app/data/ui/main-content.blp` contains two explicit “Work in progress” StatusPages. Therefore its old published screenshots cannot demonstrate current main's finished music UI. Also inspected released tag `v0.2.4`, commit `fc6c6e689bd0c60fa662b03aa2aa3c05534b4811`, [player-bar blueprint](https://altlinux.space/rirusha/Cassette/src/commit/fc6c6e689bd0c60fa662b03aa2aa3c05534b4811/data/ui/player-bar.blp) and companion `src/widgets/player-bar.vala` to distinguish real released widget structure from imagery. |

The released Cassette blueprint uses a homogeneous Grid, central CenterBox,
separate elapsed/seek/duration and a footer queue action. This corroborates the
published grouping. Its exact control order, assets, styles and code are not a
Karin template. Current main's rewrite is not evidence of a working newer queue.

## What the inspected images support

1. **Persistent music context.** The Yandex images use a quiet narrow rail and a
   footer that stays visually separate from the changing page. Cover/title/artist
   occupy the left; a centered transport group is stable; queue/volume utilities
   occupy the right. A compact central timeline connects elapsed listening to
   transport, rather than becoming an unrelated full-window horizontal rule.
2. **Queue belongs to listening.** Cassette's released queue image shows a right
   column alongside the current list and persistent footer. Active occurrence
   emphasis and compact row menus make choosing a next track understandable.
   Yandex's current help likewise treats queue/history as player operations.
   Karin will expose its existing local queue from the footer, not claim cloud
   synchronization merely because the reference service has it.
3. **Artwork before navigation cards.** Collection art, actual track information
   and readable titles should provide visual weight. Repeating the rail's
   destinations as huge rectangular shortcuts consumes useful music space.
4. **Native calm.** Cassette's released rows use consistent art/title/artist,
   quiet duration/menu and clear selection against otherwise low-decoration
   surfaces. Use native GTK themed states rather than copying its blue fill,
   fixed layout, icon paths or service assets.
5. **Bounded inspiration.** The Yandex marketing hero contains My Wave and a
   colored effect. Those unsupported features and decorative treatment are
   explicitly outside Karin's task; they are not justification for a fake hero.
   Cassette screenshots likewise show unsupported save/lyrics/wave actions;
   Karin must omit them.

## Fresh starting GTK evidence: a9eed45

QA executed the preserved exact-a9 test executable anew in isolated GTK/Xvfb,
with synthetic loopback metadata/artwork. The current audit inspected these
fresh 1× images (not reused screenshots from the preceding iteration):

| Step / image | Observed issue and implication |
| --- | --- |
| 1. `surface-home-scale1-dark.png`, 1440×900 | Three approximately 330×96px shortcut cards repeat Library/Search actions already available on the rail. HeaderBar “Главная” plus “Ваша музыка” repeats the page introduction. Main music starts near x=240 despite an 80px rail, with a roughly 160px extra gutter; small playlist tiles occupy little of the shelf and liked rows begin near y=500. Replace the shortcut band with actual collection art/actions and remove the arbitrary clamp. |
| 2. `matrix-wide-pane-open-scale1-dark.png` | A dominant full-width collection DropDown sits over a tiny collection hero. The pane and list work but the page reads as independent controls; a gap separates the constrained list and information. Use concise collection navigation, aligned real hero/list and a shared panel surface, allowing main width to grow. |
| 3. `matrix-medium-pane-open-scale1-dark.png`, 900×650 | Main row controls remain reachable, a useful property to retain. The existing footer has only three transport controls, visible Stop and a long separate full-width progress line; new modes need a deliberately regrouped footer instead of being appended into remaining gaps. |
| 4. `matrix-compact-pane-open-scale1-dark.png`, 420×720 | Dedicated information and footer are present, but HeaderBar still says “Библиотека” while the visible view is “О треке”. Compact title/Back must represent the actual view. The current small footer already needs tight width management; modes need a native compact affordance, not clipping. |

Baseline executable SHA-256:
`7e4c827ec647ea51eb612185358084e6a3ece78a4d4d6d7760735e1f1d578998`.
Fresh run images are in the QA evidence set under `before/scale1`; source and
capture fingerprints accompany the final PR evidence. Scale2 is captured
separately. These starting images show layout, not the result of the redesign.
They contain synthetic original art, not the owner's personal music library.

The owner's Fedora report establishes **partial-surface animation failure**.
A still already-open GTK frame cannot identify its exact allocation cause or
prove a fix. That requires the new 0/25/50/75/100% frame sequence, actual widget
geometry, close frames and open-mode switching with animations enabled. The
static observations above must not be presented as a temporal animation audit.

## Karin decisions grounded in that evidence

- Three primary rail destinations; footer owns Queue and Ctrl+J, without a second
  queue tree or another player. Main screen navigation and collection actions
  remain distinct.
- Natural-size genuine collection art and compact liked-music sections on Home;
  no navigation-card wall, duplicated headline or fabricated content.
- One panel mode/state and one moving surface with common header/close. Track
  metadata follows actual occurrence changes, while Queue shows the real next
  plan. Mode switching preserves the outer surface and does not replay its slide.
- Footer wings and center have explicit geometry so long titles cannot move Play.
  Shuffle/Repeat are real local policies linked to MPRIS, not aesthetic toggles.
- Use native theming/type/focus and a restrained spacing system; retain roughly
  64px song rows and legible real art. Do not copy product-specific geometry or
  replace normal artwork with a trademark/background effect.
- Resize-mode continuity, frame coherence, long metadata, actual inputs and
  runtime/cleanup regressions are acceptance criteria, not polish to defer.

See [DESIGN_SPEC.md](DESIGN_SPEC.md) for the implementation contract and
[CODEX_TASK.md](CODEX_TASK.md) for the full owner task. The implementation review
and validation record must state the actual new checks and pending owner Fedora
acceptance; neither external references nor this research mark the redesign PASS.

## Retrieval fingerprints

To identify the inspected public images without redistributing them:

```text
069c2a09bcf8064f7376d1ce7b652b6ae00a4539d6e1ccde9eb64fc19373ae7c  Yandex Windows asset
8514ab54ca77948b0bff142088f4db643ef31466ef1e0b88faccb89ae82d7aad  Yandex macOS asset
2529d919d18ea5e02a452805098f1d4600e1593991286cc266dc58fc825c96d9  Cassette main-published liked image
6b1a79d3adaad4e8ab4d81242083a4d6fb14c6f05b12ecdfb6215cefe66fe3ba  Cassette main-published queue image
b628fcec78e3089b741c041f7d5bf859ba4740b2832f01146bce5d4ff9926960  Cassette Flathub liked image
```
