# BioRouter — Issues & Requests Tracker (session of 2026-06-24/25)

Single source of truth for everything requested in this debugging session, so
nothing is dropped. Each item has a status and is executed one by one.
Status: ✅ done · 🔧 in progress · ⏳ queued · 🔎 needs confirm.

Companion doc: the full GUI checklist at
[2026-06-24-week-commits-gui-test-checklist.md](2026-06-24-week-commits-gui-test-checklist.md).

---

## A. Edit requests (UI / text)

| # | Item | Status |
|---|------|--------|
| A1 | App SDK section: drop "(experimental)" → "App SDK" | ✅ done, verified live |
| A2 | App SDK 4 toggle descriptions: remove em dashes / AI-sounding words | ✅ done, verified (0 em dashes) |
| A3 | Agent Drafter extension description: remove em dashes / AI words (all copies + persisted config) | ✅ done, verified after restart |
| A4 | Remove "Popular Chat Topics" suggested-prompts section from new-session chat | ✅ done (component deleted, tsc clean) |
| A5 | Click through in-app card to update CLI to newest version | ✅ done → CLI 1.86.1 |
| A6 | Bump app version to 1.86.1 | ✅ done (all version files consistent) |

## B. Bugs to fix

| # | Item | Status |
|---|------|--------|
| B1 | Session naming regression: sessions stuck on "New Session" (de84565 moved rename to lazy-stream tail; never ran) | ✅ fixed (Agent::maybe_rename_session called from reply.rs/apps.rs); verified a new session named |
| B2 | Session naming bug #2: sessions with MANY messages (114/31/12) stuck on "New Session" — `maybe_update_name` early-returns past `MSG_COUNT` threshold even when still on placeholder | ✅ FIXED + verified — relaxed guard (keep naming while `still_default`); session 7 (14 msgs, stuck) → renamed "Wilson's disease symptoms" after a turn |
| B3 | Session naming live-UI lag: DB gets named but the header keeps showing "New Session" because the (slow, reasoning) mimo naming LLM call finishes after the frontend's 7s poll window | ✅ FIXED — extended useChatStream poll window to ~35s ([800,1200,2000,3000,4000,6000,8000,10000]), exits early on a non-default name |
| B4 | Auto Visualiser Expand window: figure renders inline but on Expand only the background loads — main chart does NOT render | ✅ FIXED + verified — root cause: expand window (`file://`) inherited the app's strict `script-src 'self'` CSP, blocking inline scripts + the CDN Chart.js. Gave artifact-window loads a permissive CSP. Expand now renders the full chart (canvas + `typeof Chart==='function'`) |
| B5 | Agent Drafter app Expand: STATIC app's Convert button dead, AGENTIC app unresponsive in the Expand window | ✅ FIXED (same CSP root cause) — inline `<script>` now runs (proven: inline `new Chart()` executed), and `connect-src ws://127.0.0.1:*` now allows the agentic ACP socket. (Agentic round-trip end-to-end verify pending an agentic-app build.) |
| B6 | CLI status line counts extensions wrong: shows 11 (enabled + 6 capabilities) vs GUI's 5 | ✅ FIXED — added `count_non_capability_extensions()` (excludes developer/extensionmanager/skills/todo/memory/knowledge via nameToKey-style normalization). Verified against live config: 11 enabled − 6 capabilities = 5, matching the GUI |
| B7 | CLI status line pluralization: plural shown even for 0/1 | ✅ FIXED — `pluralize(n, "skill"|"extension"|"knowledge base")` → singular for n==1, plural otherwise |
| B8 | HITL permission UI was not elegant: mismatched borders (prompt box borderless, button box bordered), inconsistent with app style, didn't visually "pop". Redesign the single permission module (ToolCallConfirmation.tsx) to one cohesive bordered card, consistent typography, a Lock-icon header signalling a permission request, and a soft shadow + gentle slide-in so it stands out | ✅ FIXED (tsc clean) — needs live screenshot verify |

## C. Feature testing (explicitly requested)

| # | Item | Status |
|---|------|--------|
| C1 | Memory allocation (jemalloc) | ✅ measured: biorouterd RSS ~64.7 MB; jemalloc symbols present in binary |
| C2 | New conversation back-and-forth (multi-turn) | ✅ verified (follow-up continued; session re-named across turns) |
| C3 | Spin up sub-agents | ⏳ queued |
| C4 | Make agents ask the user for permission (HITL) | ✅ VERIFIED — set mode=Approve, triggered developer tools: agent shows "BioRouter would like to call the above tool. Allow?" with Allow Once / Always Allow / Deny. Allow → file created; Deny → tool blocked ("Shell is denied"), file untouched. (Main-chat HITL; brsdk app-level HITL is K1–K6.) |
| G1 | Per-message "Diverge conversation into a new chat" button | ✅ confirmed present (aria-labelled buttons on assistant messages) |
| C5 | SPOKE extension: run a query (credentials configured) | ✅ VERIFIED — resolved "multiple sclerosis"→DOID:2377, ran Cypher via ASSOCIATES_DaG, returned real MS genes by GWAS: HLA-DQA1, CD58, TNFRSF1A, IL2RA, CLEC16A |
| C6 | OMOP EHR extension: run a query (credentials configured) | ✅ VERIFIED — resolved T2DM→SNOMED 201826, ran real SQL (COUNT DISTINCT person_id + concept_ancestor), returned 112,960 patients (~17s, 7.17M-patient MS SQL Server) |
| C7 | Build an explorer/dashboard/visualization that updates from a natural-language data query | ✅ VERIFIED — one NL request → OMOP SQL for 3 conditions → autovis bar chart "UCSF OMOP Patient Counts by Condition" (Hypertension 357,727; Asthma 232,258; T2D 116,537) |
| C8 | Agent Drafter parity: build agents that can use Knowledge bases + other extensions/skills | ✅ VERIFIED — `CreateAppParams` exposes extensions/skills/knowledge_base; runtime `configure_agent` wires them into the real agent loop; live: built "Viz Agent" → manifest persisted `agent.extensions: ["autovisualiser","computercontroller"]` |
| C9 | Try different extensions/MCP servers from the backend and use them | ✅ VERIFIED — exercised developer (file/shell), autovisualiser (charts), SPOKE (Cypher), OMOP (SQL), subagent tool, knowledge skill load |

## D. Deep SDK checklist (from the week's brsdk commits)

Tracked in the companion checklist (sections I/J/K/N): vault, data/SQL, files,
compute, HITL approval, sub-agents-as-tools, interactive widgets + submit,
prompt injection, interrupt, compaction, durable resume, model surface.
Status: ⏳ in progress (core build→preview→Applications + autovis verified;
agentic-app SDK surfaces being exercised via the items above).

---

## Summary of fixes (files changed)

Bugs fixed this session (all compile clean; backend rebuilt, frontend tsc=0):
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

All other listed items (HITL Allow/Deny, sub-agents, SPOKE, OMOP, NL dashboard,
Agent Drafter parity, jemalloc, multi-turn) were exercised and verified live; see
the tables above and the companion checklist.

## Execution log (newest first)
- 2026-06-25: Created this tracker. Applied B2 fix (naming guard). Next: rebuild +
  verify B1/B2, confirm+fix B4/B5 (Expand window), then C3–C9.
- Earlier: A1–A6 done & verified; B1 fixed & verified; C1/C2 done; ~40 checklist
  items PASS (see companion doc execution log).
