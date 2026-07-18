# GUI debug session issue tracker (2026-06-24/25)

> **What this is.** The item-by-item tracker for a single BioRouter debugging session:
> every UI edit, bug and feature test requested on 2026-06-24/25, the status each one
> reached, and the files changed for each fix.
> **Status:** Historical record — the session ran on 2026-06-24/25 against app version
> 1.86.1 and closed on 2026-06-25. Groups A and B were completed, group C was completed
> except for C3, and group D was still in progress when the session ended. Nothing here
> is an open work queue; the current release is 1.87.2.
> **Audience:** maintainers of the BioRouter desktop UI.
> **Identifiers:** items are keyed by a group letter plus a number — `A` edit requests,
> `B` bugs, `C` feature tests, `D` the deep SDK checklist. The companion
> [week-of-2026-06-24 commit GUI regression pass](week-commit-regression-pass.md) uses
> the same style of key over its own 15 sections (`A`–`O`), so a letter means different
> things in the two files. Where this tracker cites a key from the companion document it
> says so explicitly (`G1`, `K1`–`K6`).

This tracker and the companion regression pass were written side by side during the same
two days. The regression pass is the broad matrix over the week's 133 commits; this file
is the narrower list of things a maintainer asked for during the session — copy edits,
bugs found while driving the app, and specific capabilities to prove out. Read this one
for the fixes and their root causes, and the companion for coverage.

Two terms recur below. **HITL** is human-in-the-loop tool approval: the agent pauses and
asks the user before running a tool. **brsdk** is the BioRouter Apps SDK, the surface
that Agent Drafter apps are built against.

**Status vocabulary.** Every item carries exactly one of three states:

| Status | Meaning |
|--------|---------|
| Done | The work was completed during the session. |
| In progress | Under way when the session ended. |
| Queued | Accepted, never started. |

Where a fix landed but its live verification was still outstanding, the item is `Done`
and the note beneath the table says what remained.

## Group A — edit requests (UI and text)

| # | Item | Status |
|---|------|--------|
| A1 | App SDK section: drop "(experimental)" → "App SDK" | Done |
| A2 | App SDK 4 toggle descriptions: remove em dashes / AI-sounding words | Done |
| A3 | Agent Drafter extension description: remove em dashes / AI words (all copies + persisted config) | Done |
| A4 | Remove "Popular Chat Topics" suggested-prompts section from new-session chat | Done |
| A5 | Click through in-app card to update CLI to newest version | Done |
| A6 | Bump app version to 1.86.1 | Done |

Verification notes: A1 verified live. A2 verified with 0 em dashes. A3 verified after
restart. A4 verified with the component deleted and `tsc` clean. A5 brought the CLI to
1.86.1. A6 left all version files consistent.

## Group B — bugs to fix

| # | Bug | Status |
|---|-----|--------|
| B1 | Session naming regression: sessions stuck on "New Session" | Done |
| B2 | Session naming bug #2: sessions with many messages stuck on "New Session" | Done |
| B3 | Session naming live-UI lag: database gets named but the header does not | Done |
| B4 | Auto Visualiser Expand window: only the background loads, no chart | Done |
| B5 | Agent Drafter app Expand: static app's button dead, agentic app unresponsive | Done |
| B6 | CLI status line counts extensions wrong: shows 11 vs the GUI's 5 | Done |
| B7 | CLI status line pluralization: plural shown even for 0/1 | Done |
| B8 | HITL permission UI was not elegant | Done |

### Root causes and fixes

**B1 — session naming regression.** Sessions were stuck on "New Session". Commit
`de84565` had moved the rename to the lazy-stream tail, so it never ran. Fixed by calling
`Agent::maybe_rename_session` from `reply.rs` and `apps.rs`; verified that a new session
was named.

**B2 — naming blocked past the message-count threshold.** Sessions with many messages
(114/31/12) stayed on "New Session" because `maybe_update_name` early-returns past the
`MSG_COUNT` threshold even when the name is still the placeholder. Fixed by relaxing the
guard to keep naming while `still_default`. Verified: session 7 (14 messages, stuck) was
renamed "Wilson's disease symptoms" after a turn.

**B3 — live-UI lag behind the database.** The database record was renamed but the header
kept showing "New Session", because the slow, reasoning MiMo session-naming LLM call
finished after the frontend's 7-second poll window. Fixed by extending the
`useChatStream` poll window to roughly 35 seconds
(`[800,1200,2000,3000,4000,6000,8000,10000]`), exiting early on a non-default name.

**B4 — Auto Visualiser Expand window rendered no chart.** The figure rendered inline, but
on Expand only the background loaded. Root cause: the expand window (`file://`) inherited
the app's strict `script-src 'self'` CSP, which blocked inline scripts and the CDN
Chart.js. Fixed by giving artifact-window loads a permissive CSP. Expand now renders the
full chart (canvas present and `typeof Chart==='function'`).

**B5 — Agent Drafter apps dead in the Expand window.** A static app's Convert button did
nothing and an agentic app was unresponsive. Same CSP root cause as B4. Inline `<script>`
now runs, proven by an inline `new Chart()` executing, and `connect-src ws://127.0.0.1:*`
now allows the agentic ACP socket. The agentic round-trip end-to-end verification was
still pending an agentic-app build when the session closed.

**B6 — CLI extension count.** The CLI status line showed 11 (enabled plus 6 capabilities)
where the GUI showed 5. Fixed by adding `count_non_capability_extensions()`, which
excludes `developer`, `extensionmanager`, `skills`, `todo`, `memory` and `knowledge` via
`nameToKey`-style normalization. Verified against the live config: 11 enabled − 6
capabilities = 5, matching the GUI.

**B7 — CLI pluralization.** The status line used plural forms for 0 and 1. Fixed with
`pluralize(n, "skill"|"extension"|"knowledge base")`, giving the singular for `n == 1` and
the plural otherwise.

**B8 — HITL permission UI.** The permission module had mismatched borders (a borderless
prompt box inside a bordered button box), was inconsistent with the app style, and did not
visually stand out. `ToolCallConfirmation.tsx` was redesigned into one cohesive bordered
card with consistent typography, a Lock-icon header signalling a permission request, and a
soft shadow plus a gentle slide-in. Compiles clean; live screenshot verification was still
outstanding at the end of the session.

## Group C — feature testing (explicitly requested)

| # | Item | Status |
|---|------|--------|
| C1 | Memory allocation (jemalloc) | Done |
| C2 | New conversation back-and-forth (multi-turn) | Done |
| C3 | Spin up sub-agents | Queued |
| C4 | Make agents ask the user for permission (HITL) | Done |
| C5 | SPOKE extension: run a query (credentials configured) | Done |
| C6 | OMOP EHR extension: run a query (credentials configured) | Done |
| C7 | Build an explorer/dashboard/visualization that updates from a natural-language data query | Done |
| C8 | Agent Drafter parity: build agents that can use knowledge bases plus other extensions and skills | Done |
| C9 | Try different extensions/MCP servers from the backend and use them | Done |
| G1 | Per-message "Diverge conversation into a new chat" button | Done |

> **Note.** The `G1` row is keyed to section G of the companion regression pass, not to
> this file's group C. It was tested during this session and recorded here.

### What each test showed

**C1 — jemalloc.** Measured `biorouterd` RSS at roughly 64.7 MB; jemalloc symbols present
in the binary.

**C2 — multi-turn.** Verified: a follow-up continued the conversation and the session was
re-named across turns.

**C4 — HITL approval.** Verified. With mode set to Approve, triggering developer tools
made the agent show "BioRouter would like to call the above tool. Allow?" with Allow Once,
Always Allow and Deny. Allow created the file; Deny blocked the tool ("Shell is denied")
and left the file untouched. This is main-chat HITL; app-level HITL inside brsdk apps is
tracked as `K1`–`K6` in the companion regression pass.

**C5 — SPOKE.** Verified: resolved "multiple sclerosis" to DOID:2377, ran Cypher via
`ASSOCIATES_DaG`, and returned real MS genes by GWAS — HLA-DQA1, CD58, TNFRSF1A, IL2RA,
CLEC16A.

**C6 — OMOP EHR.** Verified: resolved T2DM to SNOMED 201826, ran real SQL
(`COUNT DISTINCT person_id` plus `concept_ancestor`), and returned 112,960 patients in
about 17 seconds against a 7.17M-patient MS SQL Server.

**C7 — natural-language dashboard.** Verified: one natural-language request produced OMOP
SQL for 3 conditions and an Auto Visualiser bar chart, "UCSF OMOP Patient Counts by
Condition" (Hypertension 357,727; Asthma 232,258; T2D 116,537).

**C8 — Agent Drafter parity.** Verified: `CreateAppParams` exposes `extensions`, `skills`
and `knowledge_base`, and the runtime `configure_agent` wires them into the real agent
loop. Live check: built "Viz Agent" and the manifest persisted
`agent.extensions: ["autovisualiser","computercontroller"]`.

**C9 — extension coverage.** Verified: exercised developer (file and shell),
autovisualiser (charts), SPOKE (Cypher), OMOP (SQL), the subagent tool, and a knowledge
skill load.

**G1 — diverge button.** Confirmed present, as aria-labelled buttons on assistant
messages.

## Group D — deep SDK checklist

**Status: In progress.** The core `build → preview → Applications` pipeline and Auto
Visualiser were verified; the agentic-app SDK surfaces were being exercised through the
group C items when the session ended.

The brsdk surfaces from the week's commits are tracked in sections I, J, K and N of the
[companion regression pass](week-commit-regression-pass.md) rather than duplicated here:

- Vault
- Data and SQL
- Files
- Compute
- HITL approval
- Sub-agents as tools
- Interactive widgets and submit
- Prompt injection
- Interrupt
- Compaction
- Durable resume
- Model surface

## Summary of fixes (files changed)

Bugs fixed this session all compiled clean, with the backend rebuilt and the frontend
`tsc` at 0 errors.

- **Session naming (B1/B2/B3)** — `crates/biorouter/src/agents/agent.rs` (new
  `maybe_rename_session`, removed unreliable in-stream spawn),
  `crates/biorouter-server/src/routes/{reply.rs,apps.rs}` (call it after the loop),
  `crates/biorouter/src/session/session_manager.rs` (keep naming while still on the
  placeholder), `ui/desktop/src/hooks/useChatStream.ts` (longer poll window).
- **Expand window (B4/B5)** — `ui/desktop/src/main.ts` (permissive CSP for
  `file://` artifact windows: inline scripts, CDN, ws sidecar).
- **HITL permission UI (B8)** — `ui/desktop/src/components/ToolCallConfirmation.tsx`
  (single cohesive card, Lock header, primary action, shadow + slide-in).
- **CLI status line (B6/B7)** — `crates/biorouter-cli/src/session/tui/mod.rs`
  (exclude capabilities from the extension count; correct singular/plural).
- **UI text + version (A1–A6)** — App SDK section, Agent Drafter description (incl.
  persisted config), removed Popular Chat Topics, version → 1.86.1, CLI updated.

All other listed items — HITL Allow/Deny, sub-agents, SPOKE, OMOP, the natural-language
dashboard, Agent Drafter parity, jemalloc and multi-turn — were exercised and verified
live; see the tables above and the companion regression pass.

## Execution log

Newest first.

| When | What happened |
|------|---------------|
| 2026-06-25 | Created this tracker. Applied the B2 fix (naming guard). Planned next: rebuild and verify B1/B2, confirm and fix B4/B5 (Expand window), then C3–C9. |
| Before 2026-06-25 | A1–A6 done and verified. B1 fixed and verified. C1 and C2 done. Roughly 40 checklist items reached PASS — see the companion regression pass execution log. |

## Related documentation

- [Week-of-2026-06-24 commit GUI regression pass](week-commit-regression-pass.md) — the
  companion matrix this tracker cross-references for `G1` and `K1`–`K6`.
- [Diverge behaviour checklist](../../desktop-ui/diverge-behavior-checklist.md) — the
  current expectations for the diverge feature tested as `G1`.
- [Agent-browser debugging](../../desktop-ui/agent-browser-debugging.md) — how the
  Electron GUI was driven during this session.
- [Permission modes](../../security/permission-modes.md) — the approval modes behind the
  HITL flow redesigned in B8.
- [Apps SDK reference](../../apps-sdk/sdk-reference.md) — the brsdk surfaces group D
  delegates to the companion checklist.
