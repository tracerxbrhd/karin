<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Backlog audit — 2026-11-06

Reviewed every repository issue (#3, #5, #7, #9 and #16–#23), merged PRs and
`master` at [`ddf93ea`](https://github.com/tracerxbrhd/karin/commit/ddf93eac7184ceaddf997035bbccd94fb727200b).
Issue descriptions and historical comments are separate: old experimental goals
remain history, while current descriptions/closure records identify shipped work
and remaining criteria. No replacement issue, branch, release or tag was created.

The owner explicitly accepted corrected Dislike→Next, ordinary queue and Wave on
Fedora Native and installed Flatpak in the new task. Those reports complete
[#17](https://github.com/tracerxbrhd/karin/issues/17), now closed after merged
[#27](https://github.com/tracerxbrhd/karin/pull/27). Duplicate-occurrence,
late-result, logout and refused/unknown-write scenarios are automated regression
evidence, not newly claimed individual owner tests.

Acceptance addendum, 2026-11-11 (not a new repository-wide issue audit): the
owner accepted `c1de869` real Native and installed-Flatpak playback, complete MP3
saving, local hits and local replay after restart. Indicators are provisionally
accepted. #26 merged normally at `1fd4028`; #20 is technically complete. Prior
cache failures and the original 2026-11-06 audit remain historical evidence.

| Issue | Disposition and remaining closure condition |
| --- | --- |
| [#5](https://github.com/tracerxbrhd/karin/issues/5) | Open external gate: applicable registration/service/public-distribution basis, implemented conditions and owner-local validation. Working sign-in/audio is already acknowledged; it alone cannot close this gate. |
| [#16](https://github.com/tracerxbrhd/karin/issues/16) | Closed: Wave merged through #25 (`860672b`) and ordinary Native/installed Flatpak accepted. Removed stale pending-acceptance wording; special backend/hardware/failure cases are not inferred. |
| [#17](https://github.com/tracerxbrhd/karin/issues/17) | Closed: confirmed Dislike and automatic Next merged through #27, new owner acceptance recorded separately from CI guards. |
| [#18](https://github.com/tracerxbrhd/karin/issues/18) | Open future feature: independently establish permitted lyrics and related-track contracts, implement confirmed portions, then accept real content. Working Wave is not a related-track shelf. |
| [#19](https://github.com/tracerxbrhd/karin/issues/19) | Open acceptance: Draft PR #28 implements owned add/remove/reorder/rename/delete/visibility with exact occurrence/revision checks and GET-only uncertainty recovery; final real-service Native/installed-Flatpak acceptance is pending. |
| [#20](https://github.com/tracerxbrhd/karin/issues/20) | Completed technical cache scope: owner accepted real Native/installed-Flatpak saving, local hit and restart-local hit at `c1de869`; #26 normally merged at `1fd4028`. Indicators provisionally accepted; public service/content rights #5/#22 and future offline lifetimes remain separate. |
| [#21](https://github.com/tracerxbrhd/karin/issues/21) | Open acceptance: Draft PR #28 implements private bounded same-account context/order/position restoration without autoplay, media URLs or radio markers. Current Music validation, deliberate fresh Play/cache seek and one Native/installed-Flatpak restart acceptance remain required. |
| [#22](https://github.com/tracerxbrhd/karin/issues/22) | Open release work: #5 permission decision, declared scope/version, reproducible artifacts/source/notices and target-platform checks. Wave and Dislike are shipped, not missing features. Future #18–#21 are not automatically mandatory release scope. |
| [#23](https://github.com/tracerxbrhd/karin/issues/23) | Open technical debt: measured Cloud first-render stall, incomplete causal explanation/target-Wayland comparison. #24 is merged; ordinary tests and owner UI acceptance do not establish a cold-smoothness fix. |

Recently closed #3, #7 and #9 remain closed: the independent player/feasibility
foundation and real-player milestones were implemented through merged #4 and
#13–#15. Their remaining service rights and optional feature scope has named
follow-ups above; earlier comments remain chronological evidence, not a reason to
reopen completed milestones. No duplicate current task was found. In particular,
#5 owns registration/distribution permission, #20 owns storage/reuse and its
implementation, and #22 owns the release process.

Labels distinguish `external-blocker`, current/release `priority:P1` and later
`priority:P2` work alongside existing issue types. #19/#21 are the next combined v0.7 implementation; #18 remains separate. #23 stays open until an explained verified resolution or
an explicit owner disposition based on bounded causal evidence; no lucky faster
sample or renderer/timing workaround is an acceptance substitute.

See [storage review](AUDIO_CACHE_REVIEW.md), [architecture](ARCHITECTURE.md),
[validation](VALIDATION.md) and [limitations](LIMITATIONS.md) for technical and
acceptance boundaries. No private account data, provider correspondence, tokens,
media addresses or user screenshots are part of this audit.
