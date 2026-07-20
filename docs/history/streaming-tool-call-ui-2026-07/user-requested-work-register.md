# User-requested work register

> **What this is.** The running backlog of every issue and test directive the user
> raised during the 2026-07-19/20 QA campaign, with its status, owner commit, and
> where the evidence lives. It exists so that no instruction is lost across a long
> multi-agent session.
> **Status:** Current — updated as items land.
> **Audience:** maintainers tracking what was asked, what shipped, and what is open.

Items are worked **sequentially by priority**, not silently dropped. An item leaves
`Open` only when it has a fix commit *and* a verification record.

## Status key

`Done` — fixed, gate-proven, verified · `In progress` — agent working ·
`Open` — accepted, not started · `Needs decision` — blocked on the user.

## Register

| # | Item | Raised | Status | Evidence / commit |
|---|---|---|---|---|
| 1 | Tool calls appear already-finished; dead gap between calls | earlier | Done | streaming for 14 providers; see the campaign final report |
| 2 | Bedrock (all Anthropic models) must stream too | earlier | Done | `81294947` ConverseStream |
| 3 | "Maximum update depth exceeded" crash | earlier | Done | `d789c6ab` (pre-existing) |
| 4 | Per-token bubbles on Bedrock | earlier | Done | `d65f7103` shared message id |
| 5 | Duplicate submission on tab close | earlier | Done | `f1f1d6b6` `consumePending` wiring |
| 6 | Home-surface submit silently loses the message | earlier | Done | `6c261665` query-string transport |
| 7 | Deferred perf items 6.1b/6.2b/6.2c/6.2d | earlier | Done | `77a7564d`, `8e20f6cc`, `ae740027`, `e06a3b43` |
| 8 | Preview cannot open files the task wrote (`/tmp`) | earlier | Done | `dc66324b`, `f8f1505f` (`execute_code` wrapper) |
| 9 | Fully-Automatic mode: broad access, approval only for sensitive ops | earlier | Done | `1079f909`, `1e8fea2e`, `7bca4b5e` |
| 10 | Tool-routing taxonomy + prompt documentation | earlier | Done | `b925d72a`, `docs/agent-loop/tool-routing.md` |
| 11 | BioOKF rebuild until zero unintended tool failures | earlier | Done | 3 iterations, `90bc2acf`; iteration 3 clean |
| 12 | Markdown preview: images, hyperlinks, all GFM | earlier | Done | `3db5d420` |
| 13 | Send failure must not become a fatal session card | earlier | Done | `87a5744d` inline retryable error |
| 14 | Merge both stale branches into main; resolve conflicts | 07-19 | Done | merge `cb7238e3`, 7 conflicts |
| 15 | Documentation structure spec + conform all campaign docs | 07-19 | Done | `b540df29`, `b1270508`, `1661ab89` |
| 16 | Remove the recents count badge (counts loaded sessions, not the week) | 07-20 | Done | `211668be` |
| 17 | Alma Mater + Roche Limit: canvas must be WHITE, sidebar grey; **Alma dark unchanged**, Parchment unchanged | 07-20 | In progress | root cause: `--background-app` is a dead token (0 usages) |
| 18 | Terminal: types, splits, per-session cwd consistency, stress (busy + churn + many-at-once), PTY reaping | 07-20 | In progress | — |
| 19 | Boot animation: logo shifts between the static state and the slide-bar state; verify with vision | 07-20 | Done | `30d615f3` — splash→`ProviderGuard` handoff; 14px drop + 4px shrink, now byte-identical rects |
| 20 | Soft interrupt gives no immediate feedback (blank multi-second gap) | 07-20 | Done | `aaf24a22` — optimistic `pendingSteer` chip, retracted on echo/rejection/turn boundary |
| 21 | "Stop and Send" needs a smooth acknowledgment animation | 07-20 | Done | `aaf24a22` |
| 22 | Sending while scrolled up must smoothly return to the bottom | 07-20 | In progress | confirmed: `BaseChat.tsx` scrolls only `if (isFollowing)` |
| 23 | Parallel tool/code execution: stress + edge cases | 07-20 | Open | queued next |
| 24 | Subagents: edge cases, stress, traceability in chat | 07-20 | Open | queued next |
| 25 | Deprecation proposal for overlapping tools | 07-19 | Needs decision | `docs/agent-loop/tool-routing.md` |
| 26 | Human security review: R2-01 dynamic-path residual, MCP mutex | 07-19 | Needs decision | listed in the campaign final report |
| 27 | Docs root still holds loose files; migrate or delete, preserving all information | 07-20 | Done | already complete on this branch — see audit below |
| 28 | Recurring `code_execution` failure: "Module error: TypeError: Module could not be found" | 07-20 | In progress | root-cause + prompt/error engineering |
| 29 | GitHub issue #19 — repeated message after Cmd+T/Cmd+T, Cmd+W/Cmd+W | 07-20 | Open | keyboard path now gated by `9cc3d3a9` (revert-proven); **not closed** — see below |

## Audit: the 27 loose `docs/` root files (item 27)

Verified with git rename detection from `e78d078c` to this branch. **Nothing was
lost.** 22 files were renamed into topic folders with purpose-based names. The 5
that git reports as deletions were HTML-to-Markdown conversions or content merges,
which fall below the rename-similarity threshold:

| Deleted from the root | Where the content actually lives |
|---|---|
| `xiaomi-mimo-integration-checklist.md` | [providers/xiaomi-mimo.md](../../providers/xiaomi-mimo.md) |
| `zai-integration-checklist.md` | [providers/zai-glm.md](../../providers/zai-glm.md) |
| `github-merge-execution-plan-2026-07-13.html` | [history/branch-merge-2026-07/merge-execution-plan.md](../branch-merge-2026-07/merge-execution-plan.md) |
| `jcode-comparison-perf-analysis.md` | [history/performance-2026-06/jcode-comparison-analysis.md](../performance-2026-06/jcode-comparison-analysis.md) |
| `slack-integration-options.md` | [extensions/slack-posting-investigation.md](../../extensions/slack-posting-investigation.md) |

The root now holds exactly `README.md` and `organization.md`, as
[organization.md](../../organization.md) §8 requires.

## Issue #19: why it is gated but still open (item 29)

The reporter's repro — send a message, **Cmd+T Cmd+T**, **Cmd+W Cmd+W**, message
sent again — is the duplicate-submission defect fixed by `f1f1d6b6`, reached
through the keyboard rather than the tab strip's buttons.

**The keyboard is genuinely a different road.** Cmd+T and Cmd+W are Electron
*menu accelerators* (`main.ts`, "New Chat" / "Close Tab"), so the keystrokes are
consumed by the menu and never reach the DOM. They arrive as IPC, are answered at
the app root (`App.tsx`), and hand off through `newTabRegistry` /
`closeActiveTabRegistry` to the handlers `ChatGroupsProvider` registers. Both
roads converge on the same reducer — which was assumed rather than proven, and is
now pinned by `keyboardResubmitGuard.test.tsx` (`9cc3d3a9`).

That gate drives the real registries through the real provider, shell and
reducer. Reverting the `consumePending` dispatch fails 5 of its 6 cases, and its
three-cycle case then logs **4 submissions for 1 message** — matching the 4 user
messages measured live when the bug was first found.

**Two reasons it is not closed:**

1. **No released build contains the fix.** `v1.88.3` (the reported version) was
   tagged 2026-07-18 04:38; `f1f1d6b6` landed 2026-07-18 22:06, ~17 h later.
   `git tag --contains f1f1d6b6` is **empty** — v1.88.3 is still the newest tag,
   so the reporter cannot resolve this by upgrading today. It ships in the next
   release. Telling them otherwise would be wrong.
2. **The live GUI re-verification did not run.** Every Electron launch stalled
   immediately after the `biorouter://` registration log line, opening no window,
   under a machine load average of 190-290 from concurrent agents. The stall
   reproduced outside Playwright with a bare `electron .`, so it is environmental
   contention, not an app defect — but it means the end-to-end check (send a
   marked message, drive the menu items, count rows in `sessions.db`) is still
   outstanding. Re-run it on an idle machine before closing.

## Related documentation

- [Campaign final report](campaign-final-report.md) — per-round findings and fixes.
- [Session trace](session-trace.md) — every instruction mapped to its commit.
- [Tool errors audit](tool-errors-audit.md) — intended vs defect classification.
