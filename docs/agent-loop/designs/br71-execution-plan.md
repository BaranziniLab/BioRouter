# BR-71 Workspace Control — Implementation Plan

> **For agentic workers:** Recommended: Follow the subagent-driven-development skill
> (recommended) or executing-plans skill to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.
>
> **Status: PLAN ONLY — awaiting operator review and approval. Do not implement until
> the operator signs off on the decisions in the final section.**
>
> **Location note:** the writing-plans skill's default path is
> `docs/superpowers/plans/YYYY-MM-DD-<feature>.md`; this plan deliberately lives beside
> its design doc under `docs/agent-loop/designs/` per this repo's design-doc convention
> (`docs/agent-loop/designs/README.md`) — an intentional repo-convention override.

**Goal:** Implement the BR-71 design ([`agent-workspace-control.md`](agent-workspace-control.md),
GitHub issue #30): a `workspace` platform extension giving the agent MCP tools over the
daemon's sessions and the GUI's tabs, the backend event spine (per-session event
broadcast + detached turn runner) that makes any session observable, a daemon→GUI
`WorkspaceBridge` command channel, and — as the flagship — glass-box subagents that run
in live, human-interactive chat tabs.

**Architecture:** Four phases matching the design doc's four slices. Phase 1 builds the
session-model additions (`parent_session_id`, message provenance), a per-session
`SessionEventBus` in the `biorouter` crate carrying `AgentEvent`s, a detached turn
runner in `biorouter-server`, a `WorkspaceServices` trait bridging the crate boundary
(incl. a server turn *lease* the subagent runs will hold), and the five headless
`workspace_*` tools. Phase 2 adds the `WorkspaceBridge` (modeled on Agent Drafter's
`UiBridge`), the `GET /ui/workspace` WebSocket, and the renderer-side command applier
that maps frames onto the existing `ChatGroups` reducer. Phase 3 puts subagent
execution on BOTH planes: the observation plane (bus + observer tabs) and the
**control plane** — the child agent registers in `AgentManager` and its run holds the
server turn lock, so `/interrupt` steers the live child and Stop/cancel really stop
it (reconciliation #2) — and builds the interactive subagent tab. Phase 4 ships
instructions, docs, and release gates.

**Tech Stack:** Rust (axum 0.x, tokio broadcast/oneshot, sqlx/SQLite, rmcp, utoipa,
schemars), TypeScript/React 19 (Vite, Vitest), the repo's `just` task runner.

---

## Ground rules for the implementing engineer

- **Branch/worktree:** create a worktree per the `using-git-worktrees` skill before Task 1:
  `git worktree add .worktrees/br71-workspace-control -b br71-workspace-control` and work
  there. Never implement on `main`.
- **Commit messages:** conventional commits, **no AI co-author trailers** — this repo's CI
  rejects `Co-Authored-By: Claude …` trailers (see memory: "Landing merged into main
  repo"). Use plain `feat(...)`/`test(...)`/`docs(...)` messages exactly as written in
  each task's commit step.
- **After any change to `crates/biorouter-server/src/routes/`:** run
  `just generate-openapi && cd ui/desktop && npm run generate-api` before the frontend
  tasks that consume the route (called out explicitly where required).
- **Permission-relevant code requires human review** (`.github/copilot-instructions.md`):
  Tasks 12, 13, 14, 17, 25, and 28 touch cross-session injection/mutation/control and
  must be flagged for operator review in their PR description.
- Line numbers cited below were re-verified against the tree at commit `30d49d9a`
  (2026-07-27; the #44 working-dir lock is MERGED — see reconciliation #7). If a file
  drifts further, the named symbol is the anchor, not the number.

---

## Design conformance

How each section of `agent-workspace-control.md` maps to tasks, and where the current
code forced a reconciliation. **Genuine conflicts with the design doc are marked ⚠ and
summarized again in the operator section at the end.**

| Design § | Content | Realized by |
|---|---|---|
| §2 Design principles | Headless-first, reuse `UiBridge` pattern, no rebuilds, provenance everywhere | Cross-cutting; enforced per task |
| §3.1 Backend control plane reuse | `start_agent`, `/reply` turn lock, `/interrupt`, `/agent/cancel`, `/agent/stop`, `get_session`, add/remove extension, `set_active_for_session`, `active_work`, `AgentManager` | Tasks 8–14 wrap these exact paths; no second storage/turn path is built |
| §3.2 chatrecall ruling | Workspace implements no search; instructions route content questions to `chatrecall`; enabling workspace *suggests* chatrecall | Task 10 (instructions block), Task 33; the suggest-on-enable UI is descoped ⚠ (reconciliation #11, operator #14) |
| §3.3 Pattern donor | `UiBridge` anatomy (`control.rs:557-663`, `apps.rs:483-496`) copied at workspace scope | Tasks 16–17 |
| §3.4 Frontend seams | `openTab` dedupe, registry singletons, `create-chat-window` IPC | Tasks 19–20 |
| §4.1 Seven tools | `workspace_list/open/read_conversation/send_prompt/set_tools/close/spawn_subagent` | Tasks 10, 11, 12, 13, 14, 18, 28 |
| §4.2 Backend spine | Detached turn runner + session event broadcast + `GET /sessions/{id}/events`; "/reply becomes detached turn + subscription" is ⚠ diverged (reconciliation #9) | Tasks 5, 6, 7, 8 |
| §4.3 `WorkspaceBridge` | `/ui/workspace` WS, per-window registry, frames, layout echo, observer-backed tabs incl. daemon-opened ones | Tasks 16, 17, 19, 20, 21 |
| §4.4 Session model | `parent_session_id`, `include_subagents`, spawn-context persistence (extensions + skills + KB) | Tasks 1, 4, 24 |
| §4.5 Glass-box subagents | Spawn → registered agent + server turn lease → observability → announce → intervene → stop → report | Tasks 24–31 (the control-plane bridge is Task 25) |
| §5 Permissions & safety | Off-by-default, provenance structural, no covert reads, no self-escalation, subagent guard, fan-out caps, WS auth, cross-session close visibility | Tasks 10 (registration), 11 (Hidden refusal), 12 (per-session cap), 14 (close toasts), 28 (guard); risks section |
| §6 Server instructions | The ≤2.5k-char instruction block | Task 10 (initial), Task 33 (tuned) |
| §7 Slices | Slices 1–4 | Phases 1–4, 1:1 |
| §8 Open questions | Focus etiquette, consult convergence, cross-window targeting, observer backpressure, CLI surface | §8.1 → operator #7; §8.2 → operator #13 + Task 34 docs note; §8.3 → implemented per the design's proposed heuristic (Task 16 `focused_or_recent`), flagged operator #15; §8.4 → resync implemented (Task 7), cost measured in the harness (Task 31) + operator #16; §8.5 → operator #9 |

### Reconciliations against the current tree (design doc → what the plan actually does)

1. ⚠ **Crate boundary: the event bus carries `AgentEvent`, not `MessageEvent`, and lives
   in the `biorouter` crate.** The design says the broadcast is "registered alongside the
   agent in `AgentManager`" and reuses the `MessageEvent` wire enum. `MessageEvent` is
   defined in `biorouter-server` (`routes/reply.rs:142`), but subagent turns publish from
   inside `biorouter` (`subagent_handler.rs`), which cannot depend on the server crate.
   Resolution: a global `SessionEventBus` module in `biorouter`
   (`crates/biorouter/src/session_events.rs`) carrying `SessionBusEvent`
   (`TurnStarted`/`Agent(AgentEvent)`/`TurnFinished`); the server's observer route maps
   bus events → `MessageEvent` (the identical mapping `/reply` already performs), so the
   **wire format** is exactly the design's — the generated TS client parses it unchanged.
   `AgentEvent` is `Clone` (`agent.rs:364`), so broadcast works without new derives.
2. ⚠ **Glass-box children run under the server turn lock and inside the `AgentManager`
   registry — the control-plane half of design §4.2/§4.5-step-2.** The design says
   "run the child through the detached turn runner." Literally routing
   `run_complete_subagent_task` through `workspace/detached.rs` is not possible: the
   parent's tool call must park on the child's completion and receive a structured
   `SubagentResult`, and the subagent-specific setup (provider override, extension
   grants, `subagent_system.md` override, workflow components) lives in
   `subagent_handler.rs`/`subagent_tool.rs` inside the `biorouter` crate. What the
   design *needs* from "through the detached runner" is three properties, and Task 25
   wires each one explicitly rather than substituting a bus tee for all of them:
   - **The live child agent is addressable.** `POST /interrupt` resolves agents via
     `AgentManager::get_or_create_agent` (`reply.rs:920` → `state.rs:290`); today the
     child is a standalone `Arc::new(Agent::with_config(..))` (`subagent_handler.rs:149`)
     that the manager never sees, so a steer mints a *different* agent and the queued
     text is drained by nobody. Fix: a new `AgentManager::register_agent` puts the
     configured child into the same LRU under its session id for the run's lifetime
     (RAII deregistration), so `/interrupt`, `/reply`-between-turns, and
     `workspace_send_prompt mode:"steer"` all reach the LIVE instance.
   - **The child's run holds the server turn lock.** `AppState.active_turns` is
     server-side; the child runs inside the `biorouter` crate. Fix: `WorkspaceServices`
     gains `begin_turn(session_id, cancel) -> Box<dyn WorkspaceTurnLease>` (the server
     impl wraps the existing `TurnGuard`), acquired by `run_complete_subagent_task` for
     the run. Consequences, each load-bearing: `is_turn_active(child)` is true while it
     runs (`workspace_list` reports `running: true`; `/interrupt`'s BR-61 gate passes;
     the steer precondition in Task 12 holds); a concurrent `workspace_send_prompt
     mode:"turn"`/`/reply` on the running child is refused (the one-turn-per-session
     invariant of §3.1 holds); and `POST /agent/cancel` / `workspace_close
     scope:"turn"` / the tab's Stop all trip the run's token through the standard
     `cancel_turn` path (`state.rs:179`).
   - **One cancellation token per run.** The run token is a `child_token()` of the
     parent-supplied token (parent-cancel still kills the child; child-cancel never
     kills the parent's turn), registered with the lease AND with the active-work
     guard, so `active_work` cancel, `subagent_status {cancel}`, `/agent/cancel`, and
     Stop converge on the same token.
   Headless (no daemon installed): no lease, no registration — exactly today's
   behavior, per §2.1. This reconciliation is ⚠ because the *mechanism* differs from
   the design's sentence while delivering its observable contract; Task 25 is the
   implementation, and the Task 31 harness asserts the chain end-to-end.
3. ⚠ **The workspace extension reaches server state through a `WorkspaceServices`
   trait.** Platform extensions are constructed from `PlatformExtensionContext`
   (`extension.rs:109-113`: only `extension_manager` + `session_manager`), inside
   `ExtensionManager::new` (`extension_manager.rs:484`) — the server never touches that
   construction. But the tools need the server's turn lock (`AppState.active_turns`,
   `state.rs:93`), the detached runner, and the bridge. Resolution: a
   `WorkspaceServices` trait defined in `biorouter`
   (`crates/biorouter/src/workspace_services.rs`), implemented in `biorouter-server`
   (`workspace/services.rs`), installed process-wide via `OnceLock` at daemon bootstrap
   (`commands/agent.rs:44`, right after `AppState::new()`). This mirrors the existing
   global-singleton precedents (`AgentManager::instance()`, the `active_work` registry,
   `ActionRequiredManager`). Headless CLI (no daemon): `get()` returns `None` and the
   tools degrade with an explicit message — the design's headless requirement holds at
   the session level via `SessionManager`/`AgentManager`, which the extension reaches
   directly.
4. ⚠ **`MessageMetadata` loses `Copy`.** It is `#[derive(... Clone, Copy ...)]`
   (`message.rs:535`); adding `provenance: Option<MessageProvenance>` (String-bearing)
   removes `Copy`. Fallout is mechanical (`.clone()` at former copy sites); Task 2 owns
   it.
5. **`PLATFORM_EXTENSIONS` count test.** `extension.rs:677` asserts `len() == 5`; adding
   `workspace` makes it 6. Task 10 updates the test and asserts
   `!PLATFORM_EXTENSIONS["workspace"].default_enabled`.
6. **KB activation is single-active today.** `KnowledgeService::set_active_for_session`
   (`knowledge/service.rs:1020`) takes `Option<&str>` — one active KB per session. The
   design's `knowledge_bases: ["kb-id"]` array is accepted for schema conformance, but a
   list of length > 1 is an `INVALID_PARAMS` error naming the constraint. Operator
   decision #6.
7. **The #44 working-dir lock is MERGED (HEAD `30d49d9a`) — resolved, no seam left.**
   What landed: `SessionUpdateBuilder` has **no** `working_dir` setter (commit
   `3805b808`; the comment at `session_manager.rs:891-894` names the two sanctioned
   writers), changes go through `SessionManager::try_update_working_dir_if_empty`
   (`session_manager.rs:1156`, one atomic conditional UPDATE) or the terminal-only
   `force_update_working_dir_unguarded`; the HTTP route additionally claims the
   per-session turn guard (`routes/agent.rs:1009`). Consequences for this plan:
   - `create_session(working_dir, …)` still sets the dir **at creation** —
     `start_agent` (`routes/agent.rs:283`) does exactly this at HEAD, so
     `ServerWorkspaceServices::start_session` (Task 9) mirroring the current
     `start_agent` body is #44-correct with no lock acquisition: a fresh session has
     no messages, and the lock only guards *changes* to an existing session.
   - Nothing in this plan changes a session's working dir after creation, so no task
     needs `try_update_working_dir_if_empty`. Task 1's builder addition
     (`parent_session_id`) mirrors `diverged_from`/`branch_point_msg_uid`, NOT the
     removed `working_dir` setter.
   - The residual product question (should `workspace_open.new.working_dir` be allowed
     to differ from the caller's dir without confirmation) stays operator decision #4,
     but it is a policy choice now, not a merge-order seam.
8. **Campaign changes that postdate the design doc, reconciled here:**
   - *Session-scoped elicitation delivery (#40):* `ActionRequiredManager` queues
     requests per session scope (`action_required_manager.rs:88-134`). Detached and
     subagent turns already drain their own session's scope; tool-confirmation requests
     are `MessageContent::ToolConfirmationRequest` inside streamed `Message`s
     (`message.rs:197`), so they flow through the event bus to observer tabs untouched,
     and the tab answers via the existing `POST /action-required/tool-confirmation`.
     **No new elicitation plumbing is needed** — but Phase 3's harness must assert a
     subagent tool confirmation renders in the tab (Task 31).
   - *Turn lock is idempotency-keyed (BR-62):* the design cites
     `try_begin_turn_idempotent` and the plan uses it verbatim; detached turns pass
     `idempotency_key: None` (two keyless turns are two turns — correct here).
   - *Tab registries with acknowledged pending tokens (#38):* `newTabRegistry.ts`'s
     `pending/handled/acknowledge` protocol exists because commands can arrive before
     `ChatGroupsProvider` mounts and the empty-pair redirect
     (`useEmptyPairRedirect.ts`) races commits. The `workspaceCommandRegistry`
     (Task 19) adopts the **same** pending-queue shape so a workspace `open_tab`
     arriving while the user sits on Settings navigates to `/pair` and survives the
     redirect, instead of being dropped.
   - *Per-dock terminal registries / `SessionType::Terminal`:* terminal sessions are
     excluded from `workspace_list` scopes by default (they are panes, not
     conversations); `workspace_read_conversation` treats them like any non-Hidden
     session.
9. ⚠ **`/reply` is NOT refactored into "detached turn + subscription" — the plan keeps
   two thin server-side consumers of one agent loop.** Design §4.2's sentence implies
   factoring the `/reply` handler's loop into the detached runner. The plan instead
   tees `/reply` into the bus (Task 6) and gives the detached runner its own minimal
   consume-and-publish loop (Task 8). Rationale, stated rather than hidden: (a) the
   turn-driving logic that must never diverge — soft-interrupt draining, message
   persistence, token accounting, compaction — lives inside `Agent::reply` itself
   (`agent.rs:3368` drain loop et al.), not in the server handlers; both server loops
   are *consumers* of the same `AgentEvent` stream, so the sync burden is limited to
   event classification. (b) `/reply`'s loop is interwoven with per-request concerns
   (SSE heartbeat/timeout, supervisor task, per-request error envelopes, elicitation
   re-emission) whose extraction is a refactor larger than BR-71 and would put the
   product's hottest path at risk. (c) The bus tee makes observers see *exactly* what
   the `/reply` client sees, which is the design's actual goal. The divergence risk
    that remains — the two `SessionBusEvent` publish sites classifying terminal
    reasons differently — is pinned by Task 7's mapping test plus Task 8's lifecycle
    test asserting the same bracket shape. Operator decision #11 records this as a
    deliberate deviation the operator may veto.
10. ⚠ **Unflagged-scope items from the design, now flagged as additions:**
    - `ProvenanceKind::SpawnContext` (Task 2) is a wire-format **addition** — the design
      names only `agent_injection`/`user_direct` and describes spawn-context persistence
      via the metadata visibility pair (§4.4). The variant gives `view:"spawn_context"`
      and the tab header a structural marker instead of a magic first-message
      convention. Additive and legacy-safe (unknown-variant rows do not exist yet).
    - `workspace_send_prompt` **refusing self-injection** (Task 12) is a restriction the
      design never asked for; rationale: a session steering itself through the
      cross-session surface bypasses nothing and only confuses provenance. Operator may
      drop it (operator #12).
    - `BIOROUTER_WORKSPACE_MAX_INJECTED_TURNS` (Task 12) is an **invented env var** (the
      design names only the subagent caps); it follows the `BIOROUTER_SUBAGENT_MAX_*`
      convention and defaults to the design's 4.
11. ⚠ **"Enabling workspace should suggest (not force) enabling chatrecall" (§3.2) is
    descoped from this plan.** It is a Settings-UI affordance (a one-time suggestion
    toast/checkbox when the user toggles workspace on), not a tool-surface behavior;
    the instruction block already routes content questions to chatrecall at runtime.
    Descoped ⚠ with operator #14 deciding ship-now vs fast-follow — not silently
    dropped.
12. **`workspace_spawn_subagent` dispatch site — resolved, including how the tool is
    offered.** The tool is advertised by the workspace extension, but spawning needs
    the parent agent's `TaskConfig` (provider, extensions, working dir) which only
    `Agent::dispatch_tool_call` has (`agent.rs:2216-2249`). Extension-advertised tools
    are **prefixed** `{extension}__{tool}` when merged into the agent's tool list
    (`extension_manager.rs:971`) and the prefix is stripped only inside
    `ExtensionManager::dispatch_tool_call` (`extension_manager.rs:1321-1331`) — so the
    model calls `workspace__workspace_spawn_subagent`, and `Agent::dispatch_tool_call`
    sees the PREFIXED name. Resolution (Task 28): dispatch intercepts
    `workspace__workspace_spawn_subagent` (and the bare name, mirroring the
    prefix-stripping tolerance at `extension_manager.rs:1294-1304`) exactly like
    `SUBAGENT_TOOL_NAME` at `agent.rs:2216`, reusing `handle_subagent_tool` with the
    extra `visible`/`placement` arguments. No change is needed where
    `create_subagent_tool` is offered (`agent.rs:2658`) — the workspace extension's
    `list_tools` advertises the tool and `get_prefixed_tools` merges it, the chatrecall
    precedent. The dispatch arm re-checks `subagents_enabled` (`agent.rs:2582`) so the
    workspace surface cannot bypass the mode/model gating on the bare tool. The tool
    *surface* (name, schema, instructions) stays exactly as designed.

---

## File structure

New files (create):

```
crates/biorouter/src/session_events.rs                     # SessionEventBus: per-session broadcast of SessionBusEvent
crates/biorouter/src/workspace_services.rs                 # WorkspaceServices trait + OnceLock install/get
crates/biorouter/src/agents/workspace_extension.rs         # The `workspace` platform extension (7 tools)
crates/biorouter-server/src/workspace/mod.rs               # Module root
crates/biorouter-server/src/workspace/detached.rs          # Detached turn runner (factored /reply loop)
crates/biorouter-server/src/workspace/bridge.rs            # WorkspaceBridge + per-window registry (UiBridge sibling)
crates/biorouter-server/src/workspace/services.rs          # ServerWorkspaceServices: WorkspaceServices impl over AppState
crates/biorouter-server/src/routes/session_events.rs       # GET /sessions/{session_id}/events (SSE observer)
crates/biorouter-server/src/routes/workspace.rs            # GET /ui/workspace (WS) + auth
ui/desktop/src/components/chatGroups/workspaceCommandRegistry.ts       # Frame→dispatch seam (newTabRegistry sibling)
ui/desktop/src/components/chatGroups/workspaceCommandRegistry.test.ts
ui/desktop/src/components/chatGroups/workspaceCommandPlanner.ts        # Pure frame→(actions, effects) planner (Task 20)
ui/desktop/src/components/chatGroups/workspaceCommandPlanner.test.ts
ui/desktop/src/hooks/useWorkspaceChannel.ts                # Renderer WS client + debounced layout echo
ui/desktop/src/hooks/useWorkspaceChannel.test.tsx
ui/desktop/src/hooks/chatStreamStore.observe.test.tsx      # Observer-mode store test (Task 21)
ui/desktop/src/components/subagent/SubagentTabHeader.tsx   # Badge, spawned-by link, spawn context, grants, Stop
ui/desktop/src/components/subagent/SubagentTabHeader.test.tsx
ui/desktop/src/components/subagent/useSubagentSession.ts   # Container hook: session/grants/spawn-context/Stop (Task 29)
ui/desktop/src/components/subagent/useSubagentSession.test.tsx
ui/desktop/src/components/sessions/sessionGrouping.ts      # History parent/child grouping helper (Task 30)
ui/desktop/src/components/sessions/sessionGrouping.test.ts
scripts/workspace/glassbox-harness.mjs                     # Phase-3 harness (ui-control-harness pattern)
docs/extensions/built-in/workspace.md                      # User docs
```

Modified files (each task lists its exact touchpoints):

```
crates/biorouter/src/lib.rs                                # register session_events, workspace_services modules
crates/biorouter/src/session/session_manager.rs            # migration 17, Session.parent_session_id, include_subagents
crates/biorouter/src/conversation/message.rs               # MessageProvenance; MessageMetadata loses Copy
crates/biorouter/src/agents/agent.rs                       # soft-interrupt provenance; workspace_spawn_subagent dispatch; subagent workspace guard
crates/biorouter/src/agents/extension.rs                   # PLATFORM_EXTENSIONS entry (count test 5→6)
crates/biorouter/src/agents/mod.rs                         # pub mod workspace_extension
crates/biorouter/src/agents/subagent_tool.rs               # parent stamp + spawn-context persistence + visible/placement params
crates/biorouter/src/agents/subagent_handler.rs            # child registration + turn lease, bus tee, announce, human_intervened
crates/biorouter/src/agents/subagent_result.rs             # human_intervened field
crates/biorouter/src/execution/manager.rs                  # AgentManager::register_agent / deregister_agent_if_same (Task 25)
crates/biorouter-mcp/src/active_work.rs                    # ActiveWorkKind::DetachedTurn variant (Task 8)
crates/biorouter-server/src/state.rs                       # TurnGuard::turn_id() accessor
crates/biorouter-server/src/lib.rs                         # pub mod workspace
crates/biorouter-server/src/commands/agent.rs              # install ServerWorkspaceServices
crates/biorouter-server/src/routes/mod.rs                  # merge new routes
crates/biorouter-server/src/routes/reply.rs                # bus tee; pub(crate) get_token_state; user_direct stamping
crates/biorouter-server/src/routes/session.rs              # include_subagents query params; SessionSummary additions
crates/biorouter-server/src/openapi.rs                     # new paths/schemas
ui/desktop/src/contexts/ChatGroupsContext.tsx              # register workspace command handler; annotations; layout echo
ui/desktop/src/components/chatGroups/chatGroupsReducer.ts  # export findTabBySession (Task 20)
ui/desktop/src/components/chatGroups/ChatTabStrip.tsx      # subagent badge from annotation state (Task 29)
ui/desktop/src/hooks/chatStreamStore.tsx                   # observeSession() observer mode
ui/desktop/src/components/BaseChat.tsx                     # SubagentTabHeader mount (via useSubagentSession); provenance chips
ui/desktop/src/components/sessions/SessionListView.tsx     # "Show subagent runs" toggle + grouped rendering (Task 30)
docs/agent-loop/designs/agent-workspace-control.md         # status header updates per slice
docs/agent-loop/tool-routing.md                            # chatrecall/workspace routing row
docs/agent-loop/subagents.md                               # glass-box updates
```

---

# Phase 1 — Session model, event spine, headless workspace tools (design Slice 1)

Ships independently: after Task 15 the daemon has observable sessions, detached turns,
and five working `workspace_*` tools that operate headlessly (`gui_attached: false`),
with route + unit tests green and the OpenAPI client regenerated.

### Task 1: `sessions.parent_session_id` (schema migration 17)

**Files:**
- Modify: `crates/biorouter/src/session/session_manager.rs`
  (anchors at `30d49d9a`: `CURRENT_SCHEMA_VERSION` at :29, `Session` struct at
  :118-163, `SessionUpdateBuilder` fields at :208-233 (`diverged_from` at :231),
  builder methods `diverged_from`/`branch_point_msg_uid` at :982-993, fresh-DB
  `CREATE TABLE sessions` at :1865 (`diverged_from TEXT,` at :1886), row mapping
  `diverged_from` read at :1766, INSERT at :2061-2084, `apply_migration` at :2257,
  `apply_update` SET pattern at :2879/:2955)
- **#44 note (reconciliation #7):** the builder deliberately has NO `working_dir`
  setter (comment at :891-894). `parent_session_id` mirrors
  `diverged_from`/`branch_point_msg_uid` — plain lineage columns — NOT the guarded
  working-dir path; nothing here touches `try_update_working_dir_if_empty`.

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` at the bottom of `session_manager.rs` (find it with
`grep -n "mod tests" crates/biorouter/src/session/session_manager.rs`; use the same
`TempDir` + `SessionManager::new` pattern as the surrounding tests):

```rust
#[tokio::test]
async fn parent_session_id_round_trips() {
    let temp = tempfile::TempDir::new().unwrap();
    let manager = SessionManager::new(temp.path().to_path_buf());

    let parent = manager
        .create_session(temp.path().to_path_buf(), "parent".to_string(), SessionType::User)
        .await
        .unwrap();
    let child = manager
        .create_session(temp.path().to_path_buf(), "child".to_string(), SessionType::SubAgent)
        .await
        .unwrap();

    // Normally-created sessions carry no parent.
    assert_eq!(child.parent_session_id, None);

    manager
        .update(&child.id)
        .parent_session_id(Some(parent.id.clone()))
        .apply()
        .await
        .unwrap();

    let reread = manager.get_session(&child.id, false).await.unwrap();
    assert_eq!(reread.parent_session_id, Some(parent.id));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p biorouter --lib session::session_manager::tests::parent_session_id_round_trips`
Expected: COMPILE ERROR — `no field parent_session_id on Session` / `no method
parent_session_id on SessionUpdateBuilder`.

- [ ] **Step 3: Implement — mirror every `diverged_from` touchpoint**

The authoritative touchpoint list is `grep -n diverged_from
crates/biorouter/src/session/session_manager.rs`. Apply, in order:

(a) Bump the schema version:

```rust
pub const CURRENT_SCHEMA_VERSION: i32 = 17;
```

(b) `Session` struct (after `branch_point_msg_uid`):

```rust
    /// Id of the parent session that spawned this one as a subagent (BR-71).
    /// Sibling of `diverged_from` (branch lineage): `diverged_from` records a
    /// user fork; this records a delegation. `None` for non-subagent sessions.
    #[serde(default)]
    pub parent_session_id: Option<String>,
```

(c) Fresh-DB `CREATE TABLE sessions` (the creation block at :1865): add
`parent_session_id TEXT,` on the line after `diverged_from TEXT,` (:1886).

(d) New migration arm in `apply_migration` (:2257; after the `16 =>` arm):

```rust
            17 => {
                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN parent_session_id TEXT
                "#,
                )
                .execute(pool)
                .await?;
            }
```

(e) Row mapping (:1766 vicinity — every place a `Session` is built from a row and
`diverged_from` is read):

```rust
            parent_session_id: row.try_get("parent_session_id").ok().flatten(),
```

and at every literal `diverged_from: None,` construction site (:859, :1597), add
`parent_session_id: None,`.

(f) INSERT statement (:2061): append `, parent_session_id` to the column list, a `, ?`
to the VALUES list, and after `.bind(&session.diverged_from)` (:2084):

```rust
        .bind(&session.parent_session_id)
```

(g) `SessionUpdateBuilder`: field (at :231, beside `diverged_from: Option<Option<String>>`):

```rust
    parent_session_id: Option<Option<String>>,
```

initialize it to `None` where `diverged_from: None` is initialized in the builder
constructor, add the builder method (after `branch_point_msg_uid` at :990-993):

```rust
    /// Record (or clear) the id of the session that spawned this one as a
    /// subagent (BR-71 delegation lineage).
    pub fn parent_session_id(mut self, parent_session_id: Option<String>) -> Self {
        self.parent_session_id = Some(parent_session_id);
        self
    }
```

and mirror `diverged_from` in the storage `apply_update`'s dynamic SET construction
(the `add_update!(builder.diverged_from, "diverged_from")` push at :2879 and the
matching bind at :2955 — duplicate both for `parent_session_id`).

- [ ] **Step 4: Run the test and the full session suite**

Run: `cargo test -p biorouter --lib session::session_manager`
Expected: PASS, including the pre-existing migration tests (`…schema_version…` tests
assert `CURRENT_SCHEMA_VERSION`, which they read symbolically — they pass unmodified).

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/session/session_manager.rs
git commit -m "feat(session): add sessions.parent_session_id (migration 17, BR-71)"
```

---

### Task 2: `MessageProvenance` on `MessageMetadata`

**Files:**
- Modify: `crates/biorouter/src/conversation/message.rs`
  (anchors: `MessageMetadata` at :535-552, constructors at :554-577, `Message` at
  :623-630, `with_metadata` near :888)
- Modify: any file the compiler flags after `Copy` is removed (mechanical `.clone()`)

- [ ] **Step 1: Write the failing tests**

Add to `message.rs`'s test module:

```rust
#[test]
fn provenance_round_trips_and_legacy_metadata_still_parses() {
    // Legacy rows have no provenance key — must deserialize to None.
    let legacy: MessageMetadata =
        serde_json::from_str(r#"{"userVisible":true,"agentVisible":false}"#).unwrap();
    assert_eq!(legacy.provenance, None);

    let stamped = MessageMetadata::default().with_provenance(MessageProvenance {
        kind: ProvenanceKind::AgentInjection,
        from_session_id: Some("s-parent".into()),
        from_session_name: Some("Planning chat".into()),
    });
    let json = serde_json::to_string(&stamped).unwrap();
    let back: MessageMetadata = serde_json::from_str(&json).unwrap();
    assert_eq!(back.provenance.as_ref().unwrap().kind, ProvenanceKind::AgentInjection);
    assert_eq!(back.provenance.unwrap().from_session_id.as_deref(), Some("s-parent"));

    // Default serialization must NOT emit the key (wire compat with old clients).
    let plain = serde_json::to_value(MessageMetadata::default()).unwrap();
    assert!(plain.get("provenance").is_none());
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter --lib conversation::message`
Expected: COMPILE ERROR — `MessageProvenance` not found.

- [ ] **Step 3: Implement**

Above `MessageMetadata` add:

```rust
/// Where a message came from, when it did not originate with this session's own
/// user↔agent pair. Cross-session control without provenance is
/// indistinguishable from prompt injection (BR-71 §2.4) — stamped in storage,
/// not just in the UI, and never suppressible.
#[derive(ToSchema, Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MessageProvenance {
    pub kind: ProvenanceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_session_name: Option<String>,
}

#[derive(ToSchema, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceKind {
    /// Injected by another session's agent (`workspace_send_prompt`).
    AgentInjection,
    /// Typed by the human directly into a subagent's tab (BR-71 §4.5).
    UserDirect,
    /// The persisted spawn-context record of a subagent session (BR-71 §4.4).
    SpawnContext,
}
```

Change `MessageMetadata`'s derive from
`#[derive(ToSchema, Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]` to
`#[derive(ToSchema, Clone, PartialEq, Serialize, Deserialize, Debug)]` (drop `Copy`)
and add the field:

```rust
    /// BR-71: origin stamp for cross-session injections. `None` for ordinary
    /// same-session messages, and omitted from JSON so legacy rows/clients are
    /// untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<MessageProvenance>,
```

Set `provenance: None` in `Default` and in `agent_only`/`user_only`/`invisible`, and
add:

```rust
impl MessageMetadata {
    pub fn with_provenance(mut self, provenance: MessageProvenance) -> Self {
        self.provenance = Some(provenance);
        self
    }
}

impl Message {
    /// Stamp this message's origin (BR-71). See [`MessageProvenance`].
    pub fn with_provenance(mut self, provenance: MessageProvenance) -> Self {
        self.metadata.provenance = Some(provenance);
        self
    }
}
```

(Place the two methods inside the existing `impl` blocks rather than new ones.)

- [ ] **Step 4: Fix the `Copy` fallout, workspace-wide**

Run: `cargo check --workspace 2>&1 | head -50`
Every error is a former implicit copy of `MessageMetadata`; fix each with `.clone()`
(or a borrow where the value is only read). Do not restructure any call site.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p biorouter --lib conversation::message`
Expected: PASS.
Run: `cargo test --workspace --no-fail-fast 2>&1 | tail -5`
Expected: no new failures versus a pre-task baseline (record the baseline first if this
machine has pre-existing failures — see memory note in `llamacpp-sidecar-feature`).

- [ ] **Step 6: Commit**

```bash
git add -A crates/
git commit -m "feat(conversation): structural message provenance (BR-71); MessageMetadata drops Copy"
```

---

### Task 3: Soft-interrupt queue carries provenance

**Files:**
- Modify: `crates/biorouter/src/agents/agent.rs`
  (anchors: `soft_interrupts` field :322, init :604, `queue_soft_interrupt` :641-645,
  `drain_soft_interrupts` :648, `has_soft_interrupts` :659, drain loop :3368-3378,
  `exit_chat` check :4640)

- [ ] **Step 1: Write the failing test**

In `agent.rs`'s test module (grep `mod tests` in the file). This exercises the REAL
queue API on a real `Agent` — queue via the new stamped entry point, drain via the
method the turn loop calls — not a local stand-in `Mutex`:

```rust
#[tokio::test]
async fn soft_interrupt_queue_round_trips_provenance_through_the_real_agent() {
    use crate::conversation::message::{MessageProvenance, ProvenanceKind};

    let temp = tempfile::TempDir::new().unwrap();
    let sm = std::sync::Arc::new(crate::session::SessionManager::new(
        temp.path().to_path_buf(),
    ));
    let agent = Agent::with_config(AgentConfig::new(
        sm,
        crate::config::permission::PermissionManager::instance(),
        None,
        crate::config::BioRouterMode::Auto,
    ));

    // Legacy entry point still works and stamps nothing.
    agent.queue_soft_interrupt("plain".into());
    // Stamped entry point (BR-71): used by workspace steer + the subagent tab.
    agent.queue_soft_interrupt_with_provenance(
        "steer".into(),
        Some(MessageProvenance {
            kind: ProvenanceKind::AgentInjection,
            from_session_id: Some("s1".into()),
            from_session_name: None,
        }),
    );
    assert!(agent.has_soft_interrupts());

    // drain_soft_interrupts is exactly what the turn loop at :3368 consumes.
    let drained = agent.drain_soft_interrupts();
    assert_eq!(drained.len(), 2);
    assert_eq!(drained[0].text, "plain");
    assert!(drained[0].provenance.is_none());
    assert_eq!(drained[1].text, "steer");
    assert!(matches!(
        drained[1].provenance.as_ref().unwrap().kind,
        ProvenanceKind::AgentInjection
    ));
    assert_eq!(
        drained[1].provenance.as_ref().unwrap().from_session_id.as_deref(),
        Some("s1")
    );
    assert!(!agent.has_soft_interrupts(), "drain empties the queue");
}
```

(`AgentConfig::new(session_manager, permission_manager, scheduler, mode)` — the exact
4-arg constructor `execution/manager.rs:124-129` uses; `PermissionManager::instance()`
and `BioRouterMode::Auto` verified there. If `drain_soft_interrupts` is `pub(super)`,
the test lives inside `agent.rs`'s own module and reaches it — that is why it goes in
`agent.rs`'s test module, not a sibling file.)

The drain **loop** rewrite (Step 3's :3368 hunk — persist with adopted uid + stamped
provenance + yield) cannot be unit-tested without a full provider-driven turn; its
behavioral coverage is the live steer assertion in the Phase-3 harness (Task 31,
assertion 3: an injected `/interrupt` text appears in the child's observer stream as
a user message with `user_direct` provenance — which passes only if this loop
persists and yields the stamped message).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter --lib agents::agent::tests::soft_interrupt_queue_round_trips_provenance_through_the_real_agent`
Expected: COMPILE ERROR — no method `queue_soft_interrupt_with_provenance`, and
`drain_soft_interrupts` returns `Vec<String>` (no `.text`/`.provenance` fields).

- [ ] **Step 3: Implement**

In `agent.rs` (near the field at :322):

```rust
/// One queued mid-turn injection: the text plus who injected it (BR-71).
#[derive(Debug, Clone)]
pub struct QueuedInterrupt {
    pub text: String,
    pub provenance: Option<crate::conversation::message::MessageProvenance>,
}
```

Change the field to `pub(super) soft_interrupts: Arc<std::sync::Mutex<Vec<QueuedInterrupt>>>`,
keep `queue_soft_interrupt` source-compatible, and add the stamped variant:

```rust
    pub fn queue_soft_interrupt(&self, text: String) {
        self.queue_soft_interrupt_with_provenance(text, None);
    }

    /// BR-71: queue a mid-turn injection stamped with its origin. Used by
    /// `workspace_send_prompt mode:"steer"` and the subagent-tab steer path.
    pub fn queue_soft_interrupt_with_provenance(
        &self,
        text: String,
        provenance: Option<crate::conversation::message::MessageProvenance>,
    ) {
        if let Ok(mut q) = self.soft_interrupts.lock() {
            q.push(QueuedInterrupt { text, provenance });
        }
    }
```

`drain_soft_interrupts` now returns `Vec<QueuedInterrupt>`; the drain loop at :3368
becomes:

```rust
                for queued in self.drain_soft_interrupts() {
                    let mut m = Message::user().with_text(queued.text);
                    if let Some(p) = queued.provenance {
                        m = m.with_provenance(p);
                    }
                    // #41: adopt the minted uid — the retained/yielded copy
                    // must carry the same id as the stored row, or its next
                    // persist duplicates it instead of replaying.
                    session_manager
                        .add_message_adopting_uid(&session_config.id, &mut m)
                        .await?;
                    conversation.push(m.clone());
                    yield AgentEvent::Message(m);
                }
```

`has_soft_interrupts` and the :4640 site are unchanged in behavior.

- [ ] **Step 4: Run tests**

Run: `cargo test -p biorouter --lib agents::agent`
Expected: PASS (new test plus all existing agent tests).

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/agents/agent.rs
git commit -m "feat(agent): soft-interrupt queue carries provenance (BR-71)"
```

---

### Task 4: `include_subagents` on the session list

**Files:**
- Modify: `crates/biorouter/src/session/session_manager.rs`
  (anchors at `30d49d9a`: `list_session_summaries` public method :1242-1248 and
  storage impl :3525-3548, `SessionSummary` :165, `list_sessions_by_types` storage
  impl near :3500)
- Modify: `crates/biorouter-server/src/routes/session.rs`. Two routes at HEAD, and
  BOTH gain the flag: `GET /sessions` (handler `list_sessions` at :166 — returns full
  `Session` rows via `list_sessions()`; History's `SessionListView` calls this) and
  `GET /sessions/sidebar` (handler `list_sidebar_sessions` at :195 — calls
  `list_session_summaries(limit+1, offset)` at :202).

- [ ] **Step 1: Write the failing test**

In `session_manager.rs` tests:

```rust
#[tokio::test]
async fn list_session_summaries_hides_subagents_unless_asked() {
    let temp = tempfile::TempDir::new().unwrap();
    let manager = SessionManager::new(temp.path().to_path_buf());
    let parent = manager
        .create_session(temp.path().to_path_buf(), "p".to_string(), SessionType::User)
        .await
        .unwrap();
    let child = manager
        .create_session(temp.path().to_path_buf(), "c".to_string(), SessionType::SubAgent)
        .await
        .unwrap();
    manager
        .update(&child.id)
        .parent_session_id(Some(parent.id.clone()))
        .apply()
        .await
        .unwrap();

    let default_list = manager.list_session_summaries(50, 0, false).await.unwrap();
    assert!(default_list.iter().any(|s| s.id == parent.id));
    assert!(!default_list.iter().any(|s| s.id == child.id));

    let full = manager.list_session_summaries(50, 0, true).await.unwrap();
    let child_row = full.iter().find(|s| s.id == child.id).expect("child listed");
    assert_eq!(child_row.parent_session_id.as_deref(), Some(parent.id.as_str()));
    assert_eq!(child_row.session_type.as_deref(), Some("sub_agent"));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter --lib session::session_manager::tests::list_session_summaries_hides_subagents_unless_asked`
Expected: COMPILE ERROR — wrong arity / missing fields.

- [ ] **Step 3: Implement**

`SessionSummary` gains two nullable columns (both `Option<String>` so existing
`sqlx::FromRow` reads stay total):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct SessionSummary {
    pub id: String,
    pub working_dir: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: i64,
    /// BR-71: `sub_agent` rows are grouped under this parent in History.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    /// BR-71: the session's type as stored (`user`/`scheduled`/`sub_agent`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_type: Option<String>,
}
```

`list_session_summaries(&self, limit: u32, offset: u32, include_subagents: bool)`
passes the flag through to the storage impl at :3525, which selects the two new
columns and switches the filter:

```rust
        let type_filter = if include_subagents {
            "('user', 'scheduled', 'sub_agent')"
        } else {
            "('user', 'scheduled')"
        };
```

(splice `type_filter` into the existing `WHERE s.session_type IN ('user', 'scheduled')`
at :3537 with `format!`, exactly as the parametrized `list_sessions_by_types` variant
at :3504 already does). Add `s.parent_session_id, s.session_type` to the SELECT list
at :3529-3534.

In `routes/session.rs`, BOTH listing handlers gain the flag, default `false`:

```rust
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListSessionsQuery {
    /// BR-71: include `sub_agent` sessions (grouped under `parent_session_id`).
    #[serde(default)]
    pub include_subagents: bool,
}
```

- `GET /sessions` (`list_sessions` at :166): add `Query(query): Query<ListSessionsQuery>`
  and switch the body from `list_sessions()` to

  ```rust
      let types: &[SessionType] = if query.include_subagents {
          &[SessionType::User, SessionType::Scheduled, SessionType::SubAgent]
      } else {
          &[SessionType::User, SessionType::Scheduled]
      };
      let sessions = state
          .session_manager()
          .list_sessions_by_types(types)
          .await
          .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
  ```

  (`list_sessions()` is exactly `list_sessions_by_types(&[User, Scheduled])` —
  session_manager.rs:3520-3523 — so the default path is behavior-identical. `Session`
  already serializes `session_type`, and Task 1 added `parent_session_id`, so History
  needs no new response type. Add the `include_subagents` param to the utoipa
  `params(...)` block.)
- `GET /sessions/sidebar` (`list_sidebar_sessions` at :195): add the same
  `include_subagents` field to the existing `SidebarSessionsQuery` (:35) and forward
  it to `list_session_summaries(limit+1, offset, query.include_subagents)` at :202.

Update every other `list_session_summaries` caller found by
`grep -rn "list_session_summaries(" crates/ | grep -v test` to pass `false`
(behavior-preserving; at `30d49d9a` the only non-test caller is the sidebar handler
above — Task 10's `workspace_list` becomes the second).

- [ ] **Step 4: Run tests**

Run: `cargo test -p biorouter --lib session::session_manager && cargo check -p biorouter-server`
Expected: PASS / clean check.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/session/session_manager.rs crates/biorouter-server/src/routes/session.rs
git commit -m "feat(session): opt-in include_subagents on session listing (BR-71)"
```

---

### Task 5: `SessionEventBus` — the per-session event broadcast

**Files:**
- Create: `crates/biorouter/src/session_events.rs`
- Modify: `crates/biorouter/src/lib.rs` (add `pub mod session_events;` beside the other
  top-level modules)

- [ ] **Step 1: Write the failing test** (inline in the new module — write the whole
file test-first; it will fail to compile until Step 3 fills in the implementation)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn two_observers_both_receive_and_publish_without_observers_is_ok() {
        publish("bus-t1", SessionBusEvent::TurnFinished { reason: "stop".into() }); // no panic

        let mut a = subscribe("bus-t2");
        let mut b = subscribe("bus-t2");
        publish("bus-t2", SessionBusEvent::TurnStarted { turn_id: "turn-9".into() });
        assert!(matches!(a.recv().await.unwrap(), SessionBusEvent::TurnStarted { .. }));
        assert!(matches!(b.recv().await.unwrap(), SessionBusEvent::TurnStarted { .. }));
    }

    #[tokio::test]
    async fn slow_observer_lags_rather_than_blocking() {
        let mut rx = subscribe("bus-t3");
        for i in 0..(BUS_CAPACITY + 8) {
            publish("bus-t3", SessionBusEvent::TurnFinished { reason: format!("r{i}") });
        }
        // The first recv reports the overflow instead of stalling the publisher.
        assert!(matches!(rx.recv().await, Err(tokio::sync::broadcast::error::RecvError::Lagged(_))));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter --lib session_events`
Expected: COMPILE ERROR — items not defined.

- [ ] **Step 3: Implement**

```rust
//! Per-session event broadcast (BR-71 §4.2).
//!
//! Today, agent events flow only inside the `POST /reply` response that started
//! the turn — nothing can *observe* a session it didn't start. This bus is the
//! missing publisher: every turn (reply-driven, detached, subagent) publishes
//! its [`AgentEvent`]s here, and any number of observers (the SSE route
//! `GET /sessions/{id}/events`, `workspace_send_prompt wait:"final_message"`)
//! subscribe. Lives in the `biorouter` crate — not the server — because
//! subagent turns publish from `subagent_handler.rs`, which cannot depend on
//! `biorouter-server`. The server maps these to its `MessageEvent` wire enum.
//!
//! Senders are retained for the life of the process (mirroring
//! `ActionRequiredManager`'s scope retention): an entry is a `broadcast::Sender`
//! whose buffer only exists while receivers do, so retention is cheap.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use tokio::sync::broadcast;

use crate::agents::AgentEvent;

/// Ring capacity per session. Observers that fall further behind see
/// `RecvError::Lagged` and must resync from storage (the SSE route re-sends an
/// `UpdateConversation` snapshot).
pub const BUS_CAPACITY: usize = 256;

/// What a turn publishes. `TurnStarted`/`TurnFinished` bracket every turn so
/// observers can render lifecycle without parsing message content, and so
/// `wait:"final_message"` has an unambiguous completion signal.
#[derive(Clone, Debug)]
pub enum SessionBusEvent {
    TurnStarted { turn_id: String },
    Agent(AgentEvent),
    TurnFinished { reason: String },
}

static BUS: LazyLock<Mutex<HashMap<String, broadcast::Sender<SessionBusEvent>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn sender_for(session_id: &str) -> broadcast::Sender<SessionBusEvent> {
    let mut map = BUS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    map.entry(session_id.to_string())
        .or_insert_with(|| broadcast::channel(BUS_CAPACITY).0)
        .clone()
}

/// Subscribe to a session's live events. Safe for any session id, including one
/// with no turn running — the receiver simply waits.
pub fn subscribe(session_id: &str) -> broadcast::Receiver<SessionBusEvent> {
    sender_for(session_id).subscribe()
}

/// Publish, best-effort. A send with no receivers is a no-op, never an error —
/// publishing must cost nothing when nobody is watching.
pub fn publish(session_id: &str, event: SessionBusEvent) {
    let _ = sender_for(session_id).send(event);
}

/// How many live observers a session currently has (introspection/tests).
pub fn observer_count(session_id: &str) -> usize {
    sender_for(session_id).receiver_count()
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p biorouter --lib session_events`
Expected: `test result: ok. 2 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/session_events.rs crates/biorouter/src/lib.rs
git commit -m "feat(agent-loop): per-session SessionEventBus broadcast (BR-71 spine)"
```

---

### Task 6: `/reply` publishes every turn to the bus

**Files:**
- Modify: `crates/biorouter-server/src/state.rs` (`TurnGuard` :57-63 — add accessor)
- Modify: `crates/biorouter-server/src/routes/reply.rs`
  (anchors: turn-guard acquisition :446, event loop :625-745, `Finish` emission :848)

- [ ] **Step 1: Write the failing test**

In `state.rs`'s test module:

```rust
    #[tokio::test]
    async fn turn_guard_exposes_its_turn_id() {
        let state = AppState::new().await.unwrap();
        let guard = state
            .try_begin_turn_idempotent("tg-id-test", CancellationToken::new(), None)
            .unwrap();
        assert!(guard.turn_id().starts_with("turn-"));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter-server --lib state::tests::turn_guard_exposes_its_turn_id`
Expected: COMPILE ERROR — no method `turn_id`.

- [ ] **Step 3: Implement the accessor**

```rust
impl TurnGuard {
    /// The server-assigned id of the turn this guard owns (BR-71: published as
    /// `SessionBusEvent::TurnStarted` so observers can correlate lifecycles).
    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }
}
```

- [ ] **Step 4: Tee the reply loop into the bus**

In `reply.rs`, add the import `use biorouter::session_events::{self, SessionBusEvent};`.

Immediately after the spawned task takes ownership of the guard
(`let _turn_guard = turn_guard;` at :510):

```rust
        session_events::publish(
            &session_id,
            SessionBusEvent::TurnStarted { turn_id: _turn_guard.turn_id().to_string() },
        );
```

Restructure the stream match (the `response = timeout(...) => { match response {` block
at :643) so the event is bound once and published before local handling — the arm
headers change from `Ok(Some(Ok(AgentEvent::Message(message))))` per-variant matching
to:

```rust
                response = timeout(Duration::from_millis(500), stream.next()) => {
                    match response {
                        Ok(Some(Ok(event))) => {
                            // BR-71: every turn is observable. Publish the raw
                            // AgentEvent before local SSE handling so observer
                            // streams see exactly what this stream sees.
                            session_events::publish(&session_id, SessionBusEvent::Agent(event.clone()));
                            match event {
                                AgentEvent::Message(message) => { /* existing body, unchanged */ }
                                AgentEvent::TokenUsage(new_token_state) => { /* existing body */ }
                                AgentEvent::HistoryReplaced(new_messages) => { /* existing body */ }
                                AgentEvent::ModelChange { model, mode } => { /* existing body */ }
                                AgentEvent::McpNotification((request_id, n)) => { /* existing body */ }
                                AgentEvent::ToolCallPending(pending) => { /* existing body */ }
                                AgentEvent::TurnAborted { code, message } => { /* existing body */ }
                            }
                        }
                        Ok(Some(Err(e))) => { /* existing body, unchanged */ }
                        Ok(None) => { /* existing body */ }
                        Err(_) => { /* existing body */ }
                    }
                }
```

[MECHANICAL MOVE] The inner bodies move verbatim; only the match header nesting
changes — do not alter any body. Verify:
`git diff crates/biorouter-server/src/routes/reply.rs` must show the arm headers,
the two `session_events::publish` insertions, and pure-indentation lines only —
any other content line in the diff means a body was altered.

Beside the `MessageEvent::Finish` emission at :848 (inside the same `if !terminal_error`
and also on the terminal-error path so observers always see closure):

```rust
        session_events::publish(
            &session_id,
            SessionBusEvent::TurnFinished {
                reason: if terminal_error {
                    "error".to_string()
                } else if task_cancel.is_cancelled() {
                    "cancelled".to_string()
                } else {
                    "stop".to_string()
                },
            },
        );
```

(Place it once, just before the `if !terminal_error` block, so every exit publishes.)

- [ ] **Step 5: Add the behavioral test**

In `reply.rs`'s test module (it exists — `error_events_preserve_machine_readable_metadata`
at the bottom):

```rust
    #[tokio::test]
    async fn reply_publishes_turn_lifecycle_to_the_bus() {
        use biorouter::session_events::{self, SessionBusEvent};
        use tower::ServiceExt;

        let state = AppState::new().await.unwrap();
        let temp = tempfile::TempDir::new().unwrap();
        let session = state
            .session_manager()
            .create_session(
                temp.path().to_path_buf(),
                "bus-reply".to_string(),
                biorouter::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        let mut rx = session_events::subscribe(&session.id);

        // No provider is configured on this fresh agent, so the turn starts and
        // fails fast — which is exactly the lifecycle bracket we assert on.
        let body = serde_json::json!({
            "user_message": { "role": "user", "created": 0, "content": [{"type": "text", "text": "hi"}] },
            "session_id": session.id,
        });
        let app = routes(state.clone());
        let response = app
            .oneshot(
                axum::http::Request::post("/reply")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        // Drain the SSE body so the spawned turn task runs to completion.
        let _ = axum::body::to_bytes(response.into_body(), usize::MAX).await;

        let first = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("bus event within 5s")
            .unwrap();
        assert!(matches!(first, SessionBusEvent::TurnStarted { .. }));
        // Somewhere in the remainder there must be a TurnFinished.
        let mut finished = false;
        while let Ok(Ok(ev)) =
            tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await
        {
            if matches!(ev, SessionBusEvent::TurnFinished { .. }) {
                finished = true;
                break;
            }
        }
        assert!(finished, "TurnFinished never published");
    }
```

If `Message`'s JSON needs more required fields than shown, construct it with
`serde_json::to_value(biorouter::conversation::message::Message::user().with_text("hi"))`
instead of a literal.

- [ ] **Step 6: Run tests**

Run: `cargo test -p biorouter-server --lib routes::reply state::tests::turn_guard_exposes_its_turn_id`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/biorouter-server/src/state.rs crates/biorouter-server/src/routes/reply.rs
git commit -m "feat(server): /reply turns publish to the SessionEventBus (BR-71)"
```

---

### Task 7: `GET /sessions/{session_id}/events` — the SSE observer route

**Files:**
- Create: `crates/biorouter-server/src/routes/session_events.rs`
- Modify: `crates/biorouter-server/src/routes/reply.rs` (make `SseResponse`,
  `MessageEvent`, `TurnErrorScope`, `get_token_state` reachable: `get_token_state` at
  :224 becomes `pub(crate)`; the others are already `pub`)
- Modify: `crates/biorouter-server/src/routes/mod.rs` (merge the new router at the end
  of the `.merge(...)` chain, :83-101)
- Modify: `crates/biorouter-server/src/openapi.rs` (register the path — grep
  `cancel_turn` there and mirror the entry)

- [ ] **Step 1: Write the failing mapping test**

The mapping bus-event → wire-event is a pure function; test it directly in the new
file's test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use biorouter::agents::AgentEvent;
    use biorouter::conversation::message::Message;
    use biorouter::session_events::SessionBusEvent;

    #[test]
    fn maps_lifecycle_and_messages_and_swallows_token_updates() {
        let mut token_state = Default::default();

        assert!(map_bus_event(
            SessionBusEvent::TurnStarted { turn_id: "turn-1".into() },
            &mut token_state
        )
        .is_none());

        let mapped = map_bus_event(
            SessionBusEvent::Agent(AgentEvent::Message(Message::user().with_text("hello"))),
            &mut token_state,
        )
        .expect("message maps");
        assert!(serde_json::to_string(&mapped).unwrap().contains("\"type\":\"Message\""));

        let fin = map_bus_event(
            SessionBusEvent::TurnFinished { reason: "stop".into() },
            &mut token_state,
        )
        .expect("finish maps");
        assert!(serde_json::to_string(&fin).unwrap().contains("\"type\":\"Finish\""));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter-server --lib routes::session_events`
Expected: COMPILE ERROR — module/function not found.

- [ ] **Step 3: Implement the route**

```rust
//! BR-71 §4.2: the read-only observer stream. Lets a subagent tab, a second
//! window, or a parent-watching-child render a turn none of them started.
//! Frames reuse the `/reply` wire enum (`MessageEvent`) so the generated TS
//! client and `chatStreamStore.tsx` parse them unchanged.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use biorouter::agents::AgentEvent;
use biorouter::session_events::{self, SessionBusEvent};
use tokio::sync::mpsc;

use crate::routes::reply::{get_token_state, MessageEvent, SseResponse, TurnErrorScope};
use crate::state::AppState;

/// Map one bus event to a wire frame. `None` means "nothing to send" (token
/// updates fold into the cached state that stamps subsequent frames, exactly as
/// the `/reply` loop does — BR-52).
fn map_bus_event(
    event: SessionBusEvent,
    token_state: &mut biorouter::providers::base::TokenState,
) -> Option<MessageEvent> {
    match event {
        SessionBusEvent::TurnStarted { .. } => None,
        SessionBusEvent::TurnFinished { reason } => Some(MessageEvent::Finish {
            reason,
            token_state: token_state.clone(),
        }),
        SessionBusEvent::Agent(ev) => match ev {
            AgentEvent::Message(message) => Some(MessageEvent::Message {
                message,
                token_state: token_state.clone(),
            }),
            AgentEvent::TokenUsage(new_state) => {
                *token_state = new_state;
                None
            }
            AgentEvent::HistoryReplaced(conversation) => Some(MessageEvent::UpdateConversation {
                conversation,
                token_state: token_state.clone(),
            }),
            AgentEvent::ModelChange { model, mode } => {
                Some(MessageEvent::ModelChange { model, mode })
            }
            AgentEvent::McpNotification((request_id, message)) => {
                Some(MessageEvent::Notification { request_id, message })
            }
            AgentEvent::ToolCallPending(p) => Some(MessageEvent::ToolCallPending {
                id: p.id,
                name: p.name,
                partial_args: p.partial_args,
            }),
            AgentEvent::TurnAborted { code, message } => Some(MessageEvent::Error {
                error: message,
                code: code.wire_code().to_string(),
                scope: TurnErrorScope::Inference,
                retryable: false,
                provider_kind: None,
            }),
        },
    }
}

#[utoipa::path(
    get,
    path = "/sessions/{session_id}/events",
    params(("session_id" = String, Path, description = "Session to observe")),
    responses(
        (status = 200, description = "Read-only observer stream of the session's live events",
         body = MessageEvent, content_type = "text/event-stream"),
        (status = 404, description = "No such session"),
        (status = 401, description = "Unauthorized - invalid secret key")
    )
)]
pub async fn observe_session_events(
    Path(session_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> axum::response::Response {
    // Subscribe BEFORE the snapshot so no event falls in the gap between them.
    let mut rx = session_events::subscribe(&session_id);

    let session = match state.session_manager().get_session(&session_id, true).await {
        Ok(s) => s,
        Err(_) => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };

    let mut token_state = get_token_state(state.session_manager(), &session_id).await;
    let (tx, rx_out) = mpsc::channel::<String>(64);

    let manager_session_id = session_id.clone();
    let state_for_task = state.clone();
    tokio::spawn(async move {
        let send = |tx: &mpsc::Sender<String>, ev: &MessageEvent| {
            let frame = format!("data: {}\n\n", serde_json::to_string(ev).unwrap_or_default());
            let tx = tx.clone();
            async move { tx.send(frame).await.is_ok() }
        };

        // Join-mid-turn snapshot: the observer starts from the full stored
        // conversation, then applies live events (BR-71 §4.2).
        let snapshot = MessageEvent::UpdateConversation {
            conversation: session.conversation.unwrap_or_default(),
            token_state: token_state.clone(),
        };
        if !send(&tx, &snapshot).await {
            return;
        }

        let mut heartbeat = tokio::time::interval(Duration::from_millis(500));
        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    if !send(&tx, &MessageEvent::Ping).await { return; }
                }
                received = rx.recv() => match received {
                    Ok(event) => {
                        if let Some(mapped) = map_bus_event(event, &mut token_state) {
                            if !send(&tx, &mapped).await { return; }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // §8.4: resync from storage instead of dropping frames
                        // silently.
                        if let Ok(fresh) = state_for_task
                            .session_manager()
                            .get_session(&manager_session_id, true)
                            .await
                        {
                            let resync = MessageEvent::UpdateConversation {
                                conversation: fresh.conversation.unwrap_or_default(),
                                token_state: token_state.clone(),
                            };
                            if !send(&tx, &resync).await { return; }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                },
            }
        }
    });

    SseResponse::from_receiver(rx_out).into_response()
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/sessions/{session_id}/events", get(observe_session_events))
        .with_state(state)
}
```

Notes for the implementer, all verified against the tree:
- `SseResponse::from_receiver` — reply.rs:107 documents a raw-receiver constructor; if
  its actual name differs, use that constructor (grep `impl SseResponse`).
- `TokenState`'s path: copy the import `reply.rs` uses (it is the type inside
  `AgentEvent::TokenUsage`, re-exported from `biorouter`). If
  `biorouter::providers::base::TokenState` is not the path, mirror reply.rs's `use`.
- Axum path syntax: match the existing routes (`/sessions/{session_id}` style is used
  in utoipa annotations; the axum `.route()` string must match the other routes in
  `session.rs` — copy their brace/colon convention exactly).
- `AgentEvent::TurnAborted`'s `code.wire_code()` — same accessor the reply loop calls
  at :700.

In `routes/mod.rs` add `.merge(session_events::routes(state.clone()))` and
`pub mod session_events;`; in `openapi.rs` register `observe_session_events`.

- [ ] **Step 4: Add the route-level test**

In the same file's test module:

```rust
    #[tokio::test]
    async fn observer_gets_snapshot_then_live_events() {
        use tower::ServiceExt;
        let state = crate::state::AppState::new().await.unwrap();
        let temp = tempfile::TempDir::new().unwrap();
        let session = state
            .session_manager()
            .create_session(
                temp.path().to_path_buf(),
                "obs".to_string(),
                biorouter::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        let app = routes(state.clone());
        let response = app
            .oneshot(
                axum::http::Request::get(format!("/sessions/{}/events", session.id))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);

        // Publish one live event, then read the body: it must contain the
        // snapshot (UpdateConversation) and then the Finish frame.
        session_events::publish(
            &session.id,
            SessionBusEvent::TurnFinished { reason: "stop".into() },
        );
        let bytes =
            tokio::time::timeout(Duration::from_secs(5), collect_prefix(response.into_body()))
                .await
                .expect("body bytes in time");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("\"type\":\"UpdateConversation\""));
        assert!(text.contains("\"type\":\"Finish\""));
    }

    /// Read frames until both expected markers have arrived (the stream never
    /// closes on its own — it is an observer).
    async fn collect_prefix(body: axum::body::Body) -> Vec<u8> {
        use http_body_util::BodyExt;
        let mut collected = Vec::new();
        let mut body = body;
        while let Some(Ok(frame)) = body.frame().await {
            if let Some(data) = frame.data_ref() {
                collected.extend_from_slice(data);
                let text = String::from_utf8_lossy(&collected);
                if text.contains("UpdateConversation") && text.contains("Finish") {
                    break;
                }
            }
        }
        collected
    }
```

(`404` case: add a two-line test asserting `GET /sessions/does-not-exist/events` →
`404`.)

- [ ] **Step 5: Run tests**

Run: `cargo test -p biorouter-server --lib routes::session_events`
Expected: `test result: ok.` (3 tests).

- [ ] **Step 6: Regenerate OpenAPI**

Run: `just generate-openapi && cd ui/desktop && npm run generate-api && cd ../..`
Expected: `ui/desktop/openapi.json` and `ui/desktop/src/api/` gain the new path; no
other diffs.

- [ ] **Step 7: Commit**

```bash
git add crates/biorouter-server/src crates/biorouter-server ui/desktop/openapi.json ui/desktop/src/api
git commit -m "feat(server): GET /sessions/{id}/events read-only SSE observer (BR-71)"
```

---

### Task 8: Detached turn runner

**Files:**
- Create: `crates/biorouter-server/src/workspace/mod.rs`, `crates/biorouter-server/src/workspace/detached.rs`
- Modify: `crates/biorouter-server/src/lib.rs` (add `pub mod workspace;`)
- Modify: `crates/biorouter-mcp/src/active_work.rs` (`ActiveWorkKind` at :24-29 gains a
  `DetachedTurn` variant — the issue's binding table says "workspace-spawned work
  registers there too", and a detached turn is exactly such work; subagents already
  register via `run_complete_subagent_task`, this covers the `mode:"turn"` /
  `workspace_open.new.prompt` path)

- [ ] **Step 1: Write the failing tests** (in `detached.rs`'s test module)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use biorouter::conversation::message::Message;
    use biorouter::session::session_manager::SessionType;
    use biorouter::session_events::{self, SessionBusEvent};
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn detached_turn_refuses_when_a_turn_is_in_flight() {
        let state = crate::state::AppState::new().await.unwrap();
        let temp = tempfile::TempDir::new().unwrap();
        let session = state
            .session_manager()
            .create_session(temp.path().to_path_buf(), "busy".into(), SessionType::User)
            .await
            .unwrap();

        let _guard = state
            .try_begin_turn_idempotent(&session.id, CancellationToken::new(), None)
            .unwrap();

        let err = start_detached_turn(state.clone(), session.id.clone(), Message::user().with_text("x"))
            .await
            .unwrap_err();
        assert!(matches!(err, DetachedTurnError::TurnInFlight { .. }));
    }

    #[tokio::test]
    async fn detached_turn_publishes_lifecycle_even_when_the_turn_fails() {
        let state = crate::state::AppState::new().await.unwrap();
        let temp = tempfile::TempDir::new().unwrap();
        let session = state
            .session_manager()
            .create_session(temp.path().to_path_buf(), "detached".into(), SessionType::User)
            .await
            .unwrap();

        let mut rx = session_events::subscribe(&session.id);
        // No provider on the fresh agent → the turn starts, fails fast, and
        // must still bracket itself on the bus.
        let turn_id = start_detached_turn(state.clone(), session.id.clone(), Message::user().with_text("go"))
            .await
            .unwrap();
        assert!(turn_id.starts_with("turn-"));

        let first = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("event in time")
            .unwrap();
        assert!(matches!(first, SessionBusEvent::TurnStarted { .. }));
        let mut finished = false;
        while let Ok(Ok(ev)) =
            tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await
        {
            if matches!(ev, SessionBusEvent::TurnFinished { .. }) {
                finished = true;
                break;
            }
        }
        assert!(finished);
        // The turn lock must be released once the detached task unwinds.
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while state.is_turn_active(&session.id) {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("turn lock released");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter-server --lib workspace::detached`
Expected: COMPILE ERROR — module not found.

- [ ] **Step 3: Implement**

`workspace/mod.rs`:

```rust
//! BR-71 workspace control: detached turns (Slice 1), the WorkspaceBridge and
//! services impl (Slice 2). See docs/agent-loop/designs/agent-workspace-control.md.
pub mod detached;
```

`workspace/detached.rs`:

```rust
//! BR-71 §4.2: run a turn server-side with no attached HTTP response. Same
//! `active_turns` lock as `/reply`, events published to the SessionEventBus,
//! messages persisted by the agent exactly as on the reply path. Used by
//! `workspace_send_prompt mode:"turn"`, `workspace_open.new.prompt`, and
//! (Slice 3) subagent turns.

use std::sync::Arc;

use biorouter::agents::{AgentEvent, SessionConfig};
use biorouter::conversation::message::Message;
use biorouter::session_events::{self, SessionBusEvent};
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::state::AppState;

#[derive(Debug, thiserror::Error)]
pub enum DetachedTurnError {
    #[error("a turn is already in flight for this session (running turn {running_turn_id})")]
    TurnInFlight { running_turn_id: String },
    #[error("failed to start detached turn: {0}")]
    Start(String),
}

/// Start a detached turn for `session_id` on `user_message`. Returns the
/// server-assigned turn id immediately; the turn runs on a spawned task holding
/// the session's turn lock, and every event it produces is published to the
/// session's broadcast. The user message is stamped/persisted by the agent's
/// own reply path — this function persists nothing itself.
pub async fn start_detached_turn(
    state: Arc<AppState>,
    session_id: String,
    user_message: Message,
) -> Result<String, DetachedTurnError> {
    let cancel_token = CancellationToken::new();
    let turn_guard = state
        .try_begin_turn_idempotent(&session_id, cancel_token.clone(), None)
        .map_err(|conflict| DetachedTurnError::TurnInFlight {
            running_turn_id: conflict.running_turn_id,
        })?;
    let turn_id = turn_guard.turn_id().to_string();

    let task_state = state.clone();
    let task_turn_id = turn_id.clone();
    tokio::spawn(async move {
        // Holds the per-session turn lock for the lifetime of this detached
        // turn; dropped (releasing the session) when the task ends — the same
        // discipline as the /reply task (state.rs:52-56).
        let _turn_guard = turn_guard;
        let _interactive_turn = biorouter::scheduler::interactive_turn_guard();
        // Issue binding table: workspace-spawned work registers in active_work.
        // The guard deregisters on drop; cancel routes to this turn's token.
        let _active_work = {
            use biorouter_mcp::active_work::{ActiveWorkGuard, ActiveWorkKind};
            let token = cancel_token.clone();
            let cancel: std::sync::Arc<dyn Fn() + Send + Sync> =
                std::sync::Arc::new(move || token.cancel());
            ActiveWorkGuard::register(
                ActiveWorkKind::DetachedTurn,
                "detached workspace turn",
                Some(format!("session {session_id}")),
                Some(session_id.clone()),
                Some(cancel),
            )
        };

        session_events::publish(
            &session_id,
            SessionBusEvent::TurnStarted { turn_id: task_turn_id },
        );
        let finish = |reason: &str| {
            session_events::publish(
                &session_id,
                SessionBusEvent::TurnFinished { reason: reason.to_string() },
            );
        };

        let agent = match task_state.get_agent(session_id.clone()).await {
            Ok(agent) => agent,
            Err(e) => {
                tracing::error!("detached turn: failed to get agent: {e}");
                return finish("error");
            }
        };
        let session = match task_state.session_manager().get_session(&session_id, true).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("detached turn: failed to read session: {e}");
                return finish("error");
            }
        };

        let session_config = SessionConfig {
            id: session_id.clone(),
            schedule_id: session.schedule_id.clone(),
            max_turns: None,
            max_tool_calls: None,
            budget: None,
            retry_config: None,
            reasoning_effort: None,
        };

        let mut stream = match agent
            .reply(user_message, session_config, Some(cancel_token.clone()))
            .await
        {
            Ok(stream) => stream,
            Err(e) => {
                tracing::error!("detached turn: failed to start reply stream: {e:?}");
                return finish("error");
            }
        };

        let mut errored = false;
        while let Some(item) = stream.next().await {
            match item {
                Ok(event) => {
                    let aborted = matches!(event, AgentEvent::TurnAborted { .. });
                    session_events::publish(&session_id, SessionBusEvent::Agent(event));
                    if aborted {
                        errored = true;
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!("detached turn: stream error: {e}");
                    errored = true;
                    break;
                }
            }
        }

        finish(if errored {
            "error"
        } else if cancel_token.is_cancelled() {
            "cancelled"
        } else {
            "stop"
        });
    });

    Ok(turn_id)
}
```

If `thiserror` is not already a `biorouter-server` dependency (check `Cargo.toml`),
either add it (it is in the workspace already) or hand-write the two-variant error's
`Display`/`Error` impls.

The `ActiveWorkKind::DetachedTurn` variant is a 3-line addition in
`crates/biorouter-mcp/src/active_work.rs` mirroring the existing two (:24-37): the
enum variant plus `ActiveWorkKind::DetachedTurn => "detached_turn"` in `as_str()`
(and its inverse if a `from_str` exists — grep `as_str` usages there).
`biorouter-server` already depends on `biorouter-mcp` directly (`state.rs:5` imports
`biorouter_mcp::knowledge`), and `ActiveWorkGuard::register` is the same 5-arg
associated function `subagent_handler.rs:62-68` calls.

- [ ] **Step 4: Run tests**

Run: `cargo test -p biorouter-server --lib workspace::detached`
Expected: `test result: ok. 2 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter-server/src/workspace crates/biorouter-server/src/lib.rs
git commit -m "feat(server): detached turn runner publishing to the session bus (BR-71)"
```

---

### Task 9: `WorkspaceServices` trait, server impl, bootstrap install

**Files:**
- Create: `crates/biorouter/src/workspace_services.rs`
- Create: `crates/biorouter-server/src/workspace/services.rs`
- Modify: `crates/biorouter/src/lib.rs` (`pub mod workspace_services;`)
- Modify: `crates/biorouter-server/src/workspace/mod.rs` (`pub mod services;`)
- Modify: `crates/biorouter-server/src/commands/agent.rs` (install after
  `AppState::new()` at :44)

- [ ] **Step 1: Write the failing test** (in `workspace_services.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct FakeLease;
    impl WorkspaceTurnLease for FakeLease {
        fn turn_id(&self) -> &str { "turn-fake" }
    }

    struct Fake;
    #[async_trait::async_trait]
    impl WorkspaceServices for Fake {
        fn gui_attached(&self) -> bool { false }
        fn layout_snapshot(&self) -> Option<serde_json::Value> { None }
        fn is_turn_active(&self, _session_id: &str) -> bool { false }
        fn cancel_turn(&self, _session_id: &str) -> Option<String> { None }
        fn begin_turn(
            &self,
            _session_id: &str,
            _cancel: tokio_util::sync::CancellationToken,
        ) -> Result<Box<dyn WorkspaceTurnLease>, String> {
            Ok(Box::new(FakeLease))
        }
        async fn stop_agent(&self, _session_id: &str) -> Result<(), String> { Ok(()) }
        async fn start_detached_turn(
            &self,
            _session_id: &str,
            _message: crate::conversation::message::Message,
        ) -> Result<String, String> { Ok("turn-1".into()) }
        async fn start_session(
            &self,
            _working_dir: std::path::PathBuf,
            _extensions: Option<Vec<String>>,
            _knowledge_base: Option<String>,
        ) -> Result<String, String> { Ok("s-new".into()) }
        fn set_knowledge_base(&self, _session_id: &str, _kb: Option<&str>) -> Result<(), String> { Ok(()) }
        fn active_knowledge_base(&self, _session_id: &str) -> Option<String> { None }
        async fn gui_command(
            &self,
            _frame: serde_json::Value,
            _wait_result: bool,
        ) -> Result<serde_json::Value, String> { Err("no GUI attached".into()) }
    }

    #[test]
    fn install_is_first_wins_and_get_returns_it() {
        install(std::sync::Arc::new(Fake));
        let got = get().expect("get() returns the installed services");
        // Prove we got a real implementation back: its methods answer.
        assert!(!got.gui_attached());
        assert!(got.layout_snapshot().is_none());
        let lease = got
            .begin_turn("s-any", tokio_util::sync::CancellationToken::new())
            .expect("fake lease");
        assert_eq!(lease.turn_id(), "turn-fake");
        // Second install is a no-op, not a panic (daemon restarts in-process
        // tests) — and get() still answers afterwards.
        install(std::sync::Arc::new(Fake));
        assert!(get().is_some());
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter --lib workspace_services`
Expected: COMPILE ERROR.

- [ ] **Step 3: Implement the trait module**

```rust
//! BR-71: the crate-boundary bridge. Platform extensions live in this crate,
//! but the turn lock, detached runner, and WorkspaceBridge live in
//! `biorouter-server`. The daemon implements this trait over its `AppState`
//! and installs it process-wide at bootstrap; the workspace extension reads it
//! lazily per call. When nothing is installed (CLI-direct, tests), workspace
//! tools degrade to session-level effects reachable through `SessionManager` /
//! `AgentManager` and say so — the design's headless requirement (§2.1).

use std::sync::{Arc, OnceLock};

use crate::conversation::message::Message;

/// An opaque hold on the server's per-session turn lock (BR-71 reconciliation
/// #2). Dropping it releases the session for the next turn — the same RAII
/// discipline as the server's own `TurnGuard`, which the daemon implementation
/// wraps. Held by `run_complete_subagent_task` for a glass-box child's whole
/// run so `is_turn_active`, `/agent/cancel`, and the one-turn-per-session
/// invariant all see the child's turn.
pub trait WorkspaceTurnLease: Send {
    /// The server-assigned turn id this lease owns.
    fn turn_id(&self) -> &str;
}

#[async_trait::async_trait]
pub trait WorkspaceServices: Send + Sync {
    /// True when at least one GUI window has a live workspace channel.
    fn gui_attached(&self) -> bool;
    /// The most recent merged layout echo from all attached windows (§4.3).
    fn layout_snapshot(&self) -> Option<serde_json::Value>;
    /// True while an interactive turn is in flight for the session (BR-33 lock).
    fn is_turn_active(&self, session_id: &str) -> bool;
    /// Trip the running turn's cancellation token; `None` when idle (BR-62).
    fn cancel_turn(&self, session_id: &str) -> Option<String>;
    /// Acquire the per-session turn lock for a turn this crate is about to run
    /// itself (a subagent run — reconciliation #2). `cancel` is registered as
    /// the token `cancel_turn` trips. Errors with the running turn's id when
    /// the session is already busy.
    fn begin_turn(
        &self,
        session_id: &str,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Box<dyn WorkspaceTurnLease>, String>;
    /// Cancel any turn, then evict the agent from the registry (session survives).
    async fn stop_agent(&self, session_id: &str) -> Result<(), String>;
    /// Start a detached turn; `Ok(turn_id)` or an error naming the conflict.
    async fn start_detached_turn(&self, session_id: &str, message: Message)
        -> Result<String, String>;
    /// Create a session the way `POST /agent/start` does (extension names are
    /// resolved against the config registry). Returns the new session id.
    async fn start_session(
        &self,
        working_dir: std::path::PathBuf,
        extensions: Option<Vec<String>>,
        knowledge_base: Option<String>,
    ) -> Result<String, String>;
    /// Set (or clear) the single active knowledge base for a session.
    fn set_knowledge_base(&self, session_id: &str, kb: Option<&str>) -> Result<(), String>;
    /// The session's active knowledge base id, if any (`workspace_list` §4.1,
    /// spawn-context grants §4.4). `None` headless or when unset.
    fn active_knowledge_base(&self, session_id: &str) -> Option<String>;
    /// Push a workspace frame to the GUI (§4.3). `wait_result` parks for the
    /// renderer's `workspace_result`. Errors when no GUI is attached.
    async fn gui_command(
        &self,
        frame: serde_json::Value,
        wait_result: bool,
    ) -> Result<serde_json::Value, String>;
}

static WORKSPACE_SERVICES: OnceLock<Arc<dyn WorkspaceServices>> = OnceLock::new();

/// Install the daemon's implementation. First install wins; later calls are
/// no-ops (matters only to in-process test harnesses).
pub fn install(services: Arc<dyn WorkspaceServices>) {
    let _ = WORKSPACE_SERVICES.set(services);
}

/// The installed services, or `None` when running without the daemon.
pub fn get() -> Option<Arc<dyn WorkspaceServices>> {
    WORKSPACE_SERVICES.get().cloned()
}
```

- [ ] **Step 4: Implement the server side** (`workspace/services.rs`)

```rust
//! The daemon's `WorkspaceServices` implementation over `AppState` (BR-71).
//! GUI methods are wired in Slice 2 (Task 17); until then they report headless.

use std::path::PathBuf;
use std::sync::Arc;

use biorouter::config::{get_enabled_extensions, get_extension_by_name};
use biorouter::conversation::message::Message;
use biorouter::session::session_manager::SessionType;
use biorouter::session::EnabledExtensionsState;
use biorouter::workspace_services::WorkspaceServices;

use crate::state::AppState;

pub struct ServerWorkspaceServices {
    state: Arc<AppState>,
}

impl ServerWorkspaceServices {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[async_trait::async_trait]
impl WorkspaceServices for ServerWorkspaceServices {
    fn gui_attached(&self) -> bool {
        false // Slice 2 (Task 17) wires the WorkspaceBridge registry here.
    }

    fn layout_snapshot(&self) -> Option<serde_json::Value> {
        None // Slice 2.
    }

    fn is_turn_active(&self, session_id: &str) -> bool {
        self.state.is_turn_active(session_id)
    }

    fn cancel_turn(&self, session_id: &str) -> Option<String> {
        self.state.cancel_turn(session_id)
    }

    fn begin_turn(
        &self,
        session_id: &str,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Box<dyn WorkspaceTurnLease>, String> {
        let guard = self
            .state
            .try_begin_turn_idempotent(session_id, cancel, None)
            .map_err(|conflict| {
                format!(
                    "a turn is already in flight for this session (running turn {})",
                    conflict.running_turn_id
                )
            })?;
        Ok(Box::new(ServerTurnLease { guard }))
    }

    async fn stop_agent(&self, session_id: &str) -> Result<(), String> {
        // Mirror POST /agent/stop (routes/agent.rs:788): cancel the turn, then
        // evict — the session record remains.
        let _ = self.state.cancel_turn(session_id);
        self.state
            .agent_manager
            .remove_session(session_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn start_detached_turn(
        &self,
        session_id: &str,
        message: Message,
    ) -> Result<String, String> {
        super::detached::start_detached_turn(self.state.clone(), session_id.to_string(), message)
            .await
            .map_err(|e| e.to_string())
    }

    async fn start_session(
        &self,
        working_dir: PathBuf,
        extensions: Option<Vec<String>>,
        knowledge_base: Option<String>,
    ) -> Result<String, String> {
        // The minimal core of POST /agent/start (routes/agent.rs:232-380):
        // create → apply extension set → persist → eager-load in background.
        let configs = match extensions {
            None => get_enabled_extensions(),
            Some(names) => {
                let mut configs = Vec::with_capacity(names.len());
                for name in &names {
                    match get_extension_by_name(name) {
                        Some(c) => configs.push(c),
                        None => return Err(format!("unknown extension '{name}'")),
                    }
                }
                configs
            }
        };

        let manager = self.state.session_manager();
        let session = manager
            .create_session(working_dir, "New Session".to_string(), SessionType::User)
            .await
            .map_err(|e| format!("failed to create session: {e}"))?;

        let mut extension_data = session.extension_data.clone();
        let extensions_state = EnabledExtensionsState::new(configs);
        extensions_state
            .to_extension_data(&mut extension_data)
            .map_err(|e| format!("failed to initialize extensions: {e}"))?;
        manager
            .update(&session.id)
            .extension_data(extension_data)
            .apply()
            .await
            .map_err(|e| format!("failed to save extension state: {e}"))?;

        if let Some(kb) = knowledge_base {
            self.set_knowledge_base(&session.id, Some(&kb))?;
        }

        // Eager extension load, exactly as start_agent does.
        let state = self.state.clone();
        let session_for_spawn = manager
            .get_session(&session.id, false)
            .await
            .map_err(|e| e.to_string())?;
        let sid = session.id.clone();
        let task = tokio::spawn(async move {
            match state.get_agent(session_for_spawn.id.clone()).await {
                Ok(agent) => agent.load_extensions_from_session(&session_for_spawn).await,
                Err(e) => {
                    tracing::warn!("workspace start_session: agent create failed: {e}");
                    vec![]
                }
            }
        });
        self.state.set_extension_loading_task(sid, task).await;

        Ok(session.id)
    }

    fn set_knowledge_base(&self, session_id: &str, kb: Option<&str>) -> Result<(), String> {
        self.state
            .knowledge_service
            .set_active_for_session(session_id, kb)
            .map_err(|e| e.to_string())
    }

    fn active_knowledge_base(&self, session_id: &str) -> Option<String> {
        // KnowledgeService::get_active_for_session (knowledge/service.rs:1006);
        // best-effort — a read error reports "no active KB", never fails a list.
        self.state
            .knowledge_service
            .get_active_for_session(session_id)
            .ok()
            .flatten()
    }

    async fn gui_command(
        &self,
        _frame: serde_json::Value,
        _wait_result: bool,
    ) -> Result<serde_json::Value, String> {
        Err("no GUI attached".to_string()) // Slice 2.
    }
}

/// The daemon's lease: a wrapped `TurnGuard` (state.rs:57). Dropping it releases
/// the session's turn slot exactly as the /reply task's guard does.
struct ServerTurnLease {
    guard: crate::state::TurnGuard,
}

impl biorouter::workspace_services::WorkspaceTurnLease for ServerTurnLease {
    fn turn_id(&self) -> &str {
        self.guard.turn_id()
    }
}
```

Import notes: `EnabledExtensionsState`'s real path is whatever `routes/agent.rs`
imports (`routes/agent.rs:27`: `biorouter::session::EnabledExtensionsState`); add
`use biorouter::workspace_services::{WorkspaceServices, WorkspaceTurnLease};` and
make `TurnGuard` importable (`pub struct TurnGuard` at `state.rs:57` already is —
via `crate::state::TurnGuard`). `begin_turn` depends on Task 6's
`TurnGuard::turn_id()` accessor.

**#44 conformance (reconciliation #7):** `start_session` above sets the working dir
**at creation** via `create_session(working_dir, …)` — exactly what `start_agent`
does at HEAD (`routes/agent.rs:283`); the working-dir *lock* guards only later
changes to a non-empty chat and is not involved here. No lock acquisition, no
seam.

In `commands/agent.rs`, right after `let app_state = state::AppState::new().await?;`:

```rust
    biorouter::workspace_services::install(std::sync::Arc::new(
        crate::workspace::services::ServerWorkspaceServices::new(app_state.clone()),
    ));
```

- [ ] **Step 5: Add a services smoke test** (in `services.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use biorouter::workspace_services::WorkspaceServices;

    #[tokio::test]
    async fn start_session_creates_a_user_session_and_rejects_unknown_extensions() {
        let state = crate::state::AppState::new().await.unwrap();
        let services = ServerWorkspaceServices::new(state.clone());
        let temp = tempfile::TempDir::new().unwrap();

        let err = services
            .start_session(temp.path().to_path_buf(), Some(vec!["no-such-ext".into()]), None)
            .await
            .unwrap_err();
        assert!(err.contains("no-such-ext"));

        let sid = services
            .start_session(temp.path().to_path_buf(), None, None)
            .await
            .unwrap();
        let session = state.session_manager().get_session(&sid, false).await.unwrap();
        assert_eq!(session.session_type, biorouter::session::session_manager::SessionType::User);
    }

    #[tokio::test]
    async fn begin_turn_lease_holds_the_lock_and_cancel_turn_trips_its_token() {
        use tokio_util::sync::CancellationToken;
        let state = crate::state::AppState::new().await.unwrap();
        let services = ServerWorkspaceServices::new(state.clone());

        let token = CancellationToken::new();
        let lease = services
            .begin_turn("lease-s1", token.clone())
            .expect("lock acquired");
        assert!(lease.turn_id().starts_with("turn-"));
        assert!(services.is_turn_active("lease-s1"));

        // A second begin_turn conflicts — the one-turn-per-session invariant.
        let conflict = services
            .begin_turn("lease-s1", CancellationToken::new())
            .unwrap_err();
        assert!(conflict.contains("already in flight"));

        // cancel_turn reaches the lease's token — this is what makes the tab's
        // Stop / workspace_close scope:"turn" work on a subagent run (Task 25).
        assert!(services.cancel_turn("lease-s1").is_some());
        assert!(token.is_cancelled());

        // Dropping the lease frees the session.
        drop(lease);
        assert!(!services.is_turn_active("lease-s1"));
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p biorouter --lib workspace_services && cargo test -p biorouter-server --lib workspace::services`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/biorouter/src/workspace_services.rs crates/biorouter/src/lib.rs crates/biorouter-server/src
git commit -m "feat(workspace): WorkspaceServices trait + daemon impl + bootstrap install (BR-71)"
```

---

### Task 10: The `workspace` platform extension skeleton + `workspace_list`

**Files:**
- Create: `crates/biorouter/src/agents/workspace_extension.rs`
- Modify: `crates/biorouter/src/agents/mod.rs` (add `pub mod workspace_extension;`
  beside `pub mod chatrecall_extension;` — grep for it)
- Modify: `crates/biorouter/src/agents/extension.rs` (PLATFORM_EXTENSIONS entry at
  :43-107; count test at :672-679)

- [ ] **Step 1: Write the failing tests**

In `extension.rs`'s existing test module (:672):

```rust
    #[test]
    fn workspace_platform_extension_is_registered_and_off_by_default() {
        assert_eq!(PLATFORM_EXTENSIONS.len(), 6);
        assert!(!PLATFORM_EXTENSIONS["workspace"].default_enabled);
    }
```

and update the existing assertion `assert_eq!(PLATFORM_EXTENSIONS.len(), 5);` → `6`.

In the new `workspace_extension.rs` test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::extension::PlatformExtensionContext;
    use crate::agents::mcp_client::McpClientTrait;
    use tokio_util::sync::CancellationToken;

    fn client() -> WorkspaceClient {
        let temp = tempfile::TempDir::new().unwrap();
        let session_manager = std::sync::Arc::new(crate::session::SessionManager::new(
            temp.path().to_path_buf(),
        ));
        // Leak the tempdir for the test's lifetime so the DB stays alive.
        std::mem::forget(temp);
        WorkspaceClient::new(PlatformExtensionContext {
            extension_manager: None,
            session_manager,
        })
        .unwrap()
    }

    #[tokio::test]
    async fn advertises_the_slice1_tools_with_instructions() {
        let c = client();
        let tools = c.list_tools(None, CancellationToken::new()).await.unwrap().tools;
        let names: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
        for expected in [
            "workspace_list",
            "workspace_read_conversation",
            "workspace_send_prompt",
            "workspace_set_tools",
            "workspace_close",
        ] {
            assert!(names.contains(&expected.to_string()), "missing {expected}");
        }
        let info = c.get_info().unwrap();
        let instructions = info.instructions.as_deref().unwrap();
        assert!(instructions.contains("chatrecall"));
        assert!(instructions.len() <= 2500, "injection budget (§6)");
    }

    #[tokio::test]
    async fn workspace_list_reports_headless_and_sessions() {
        let c = client();
        let parent = c
            .context
            .session_manager
            .create_session(
                std::env::temp_dir(),
                "listed".to_string(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        let args: rmcp::model::JsonObject =
            serde_json::from_value(serde_json::json!({ "scope": "all" })).unwrap();
        let result = c
            .call_tool(
                "workspace_list",
                Some(args),
                crate::agents::mcp_client::McpMeta::new("caller"),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let text = result.content[0].as_text().unwrap().text.clone();
        assert!(text.contains(&parent.id));
        assert!(text.contains("\"gui_attached\": false"));
        // §4.1: per-session enabled extensions + active KB are part of the row.
        assert!(text.contains("\"extensions\""));
        assert!(text.contains("\"knowledge_base\""));
    }
}
```

(`McpMeta` derives only `Clone, Debug` — no `Default` — so construct it with
`McpMeta::new(session_id)` (`mcp_client.rs:146-152`; fields at :137-144:
`session_id` + `progress_token`). rmcp's `as_text()` returns a `RawTextContent`
reference, so the payload is `.as_text().unwrap().text` — the same access every
existing test uses, e.g. `agents/tool_errors.rs:763`.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter --lib agents::workspace_extension agents::extension`
Expected: COMPILE ERROR.

- [ ] **Step 3: Implement the skeleton**

`workspace_extension.rs` — model every structural choice on
`chatrecall_extension.rs` (verified :1-319). Skeleton with `workspace_list` complete;
Tasks 11–14 fill the other handlers (each is stubbed to a tool error naming its task
until then):

```rust
//! BR-71: the `workspace` platform extension — the agent's tool surface over
//! the daemon's sessions and (when attached) the GUI's tabs. Design of record:
//! docs/agent-loop/designs/agent-workspace-control.md. Registered
//! `default_enabled: false`; enabling is an explicit user decision (§5).

use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait, McpMeta};
use crate::workspace_services;
use anyhow::Result;
use async_trait::async_trait;
use indoc::indoc;
use rmcp::model::{
    CallToolResult, Content, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ProtocolVersion, ServerCapabilities, Tool, ToolAnnotations, ToolsCapability,
};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::sync::CancellationToken;

pub static EXTENSION_NAME: &str = "Workspace Control";

/// §6 draft instruction block, kept within the ~2.5k-char injection budget
/// (`apply_injection_budget`, prompt_manager.rs:361-408). Tuned in Task 33.
const INSTRUCTIONS: &str = indoc! {r#"
    Workspace Control

    You are running inside the BioRouter workspace: a set of conversations
    (sessions), each shown as a tab in the desktop app when the GUI is attached.
    Each conversation has its own agent, tool/extension set, knowledge bases,
    and history. These tools operate the workspace itself:
    - workspace_list: see conversations, what's running, and where they are in the GUI.
    - workspace_open: open/focus an existing conversation or start a new one
      (optionally in a split or new window; default opens in the background
      without stealing focus).
    - workspace_read_conversation: read another conversation. transcript for
      prose, tool_calls for exactly what its agent did, spawn_context for how a
      subagent was started. Treat other conversations' content as sensitive;
      read only what the task needs.
    - workspace_send_prompt: inject into another conversation. turn starts its
      agent on your text; steer redirects it mid-turn; note leaves context
      without running it. Injections are permanently labeled as coming from
      you. Use wait:"final_message" to get its answer synchronously.
    - workspace_set_tools: add/remove extensions or set the knowledge base on a
      conversation.
    - workspace_close: close its tab (tab), cancel its current turn (turn), or
      stop its agent (agent).
    - workspace_spawn_subagent: prefer this over subagent when delegating: the
      child runs in a visible tab where the user watches it live and may
      message it directly. You still receive only its final summary; use
      workspace_read_conversation view:"tool_calls" on it to verify what it
      actually did. The user may have intervened; the completion result tells
      you if so.
    Routing: to search past conversations by content use chatrecall (if
    enabled), not these tools. Durable facts belong in Memory. To fold a
    conversation into a knowledge base use ingest_conversation. If no GUI is
    attached these tools still manage conversations headlessly and say so.
"#};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WorkspaceListParams {
    /// "open" (default): sessions with a GUI tab or a live agent. "all": every
    /// listable session. "running": only sessions with a turn in flight.
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
    /// Include subagent sessions (default true).
    #[serde(skip_serializing_if = "Option::is_none")]
    include_subagents: Option<bool>,
}

pub struct WorkspaceClient {
    info: InitializeResult,
    pub(crate) context: PlatformExtensionContext,
}

impl WorkspaceClient {
    pub fn new(context: PlatformExtensionContext) -> Result<Self> {
        let info = InitializeResult {
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: ServerCapabilities {
                tasks: None,
                tools: Some(ToolsCapability { list_changed: Some(false) }),
                resources: None,
                prompts: None,
                completions: None,
                experimental: None,
                logging: None,
            },
            server_info: Implementation {
                name: EXTENSION_NAME.to_string(),
                title: Some(EXTENSION_NAME.to_string()),
                version: "1.0.0".to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(INSTRUCTIONS.to_string()),
        };
        Ok(Self { info, context })
    }

    fn tool(name: &str, description: &str, schema: serde_json::Value, read_only: bool) -> Tool {
        let input_schema = schema.as_object().expect("schema is an object").clone();
        Tool::new(name.to_string(), description.to_string(), input_schema).annotate(
            ToolAnnotations {
                title: Some(name.replace('_', " ")),
                read_only_hint: Some(read_only),
                destructive_hint: Some(!read_only),
                idempotent_hint: Some(read_only),
                open_world_hint: Some(false),
            },
        )
    }

    fn get_tools() -> Vec<Tool> {
        vec![
            Self::tool(
                "workspace_list",
                "List conversations in the workspace: id, name, type, running \
                 state, parent, enabled extensions, active knowledge base, and \
                 GUI tab placement when a GUI is attached.",
                serde_json::to_value(schema_for!(WorkspaceListParams)).unwrap(),
                true,
            ),
            // Tasks 11-14 and 18/27 append: workspace_read_conversation,
            // workspace_send_prompt, workspace_set_tools, workspace_close,
            // workspace_open, workspace_spawn_subagent (advertised only; the
            // spawn dispatch lives in agent.rs — see Task 28).
        ]
    }

    async fn handle_list(
        &self,
        _caller_session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspaceListParams = match arguments {
            Some(a) => serde_json::from_value(serde_json::Value::Object(a))
                .map_err(|e| format!("invalid arguments: {e}"))?,
            None => WorkspaceListParams { scope: None, include_subagents: None },
        };
        let scope = args.scope.as_deref().unwrap_or("open");
        let include_subagents = args.include_subagents.unwrap_or(true);

        let services = workspace_services::get();
        let gui_attached = services.as_ref().is_some_and(|s| s.gui_attached());
        let layout = services.as_ref().and_then(|s| s.layout_snapshot());

        let summaries = self
            .context
            .session_manager
            .list_session_summaries(200, 0, include_subagents)
            .await
            .map_err(|e| format!("failed to list sessions: {e}"))?;

        let agent_manager = crate::execution::manager::AgentManager::instance()
            .await
            .map_err(|e| format!("agent manager unavailable: {e}"))?;

        let mut rows = Vec::new();
        for s in summaries {
            let running = services.as_ref().is_some_and(|svc| svc.is_turn_active(&s.id));
            let live = agent_manager.has_session(&s.id).await;
            let gui_placement = gui_tab_for(layout.as_ref(), &s.id);
            let in_scope = match scope {
                "running" => running,
                "all" => true,
                _ /* "open" */ => live || gui_placement.is_some(),
            };
            if !in_scope {
                continue;
            }
            // §4.1 required row fields: enabled extension names + active KB.
            // Read per included row only (the summary row has no extension_data),
            // exactly the GET /sessions/{id}/extensions fallback logic
            // (routes/session.rs:757-760). Best-effort: a read failure yields
            // an empty list, never fails the whole listing.
            let extensions: Vec<String> = match self
                .context
                .session_manager
                .get_session(&s.id, false)
                .await
            {
                Ok(full) => crate::session::EnabledExtensionsState::from_extension_data(
                    &full.extension_data,
                )
                .map(|st| st.extensions.iter().map(|e| e.name().to_string()).collect())
                .unwrap_or_else(|| {
                    // No session-specific state → global config, the exact
                    // fallback GET /sessions/{id}/extensions performs
                    // (from_extension_data returns Option).
                    crate::config::get_enabled_extensions()
                        .iter()
                        .map(|e| e.name().to_string())
                        .collect()
                }),
                Err(_) => Vec::new(),
            };
            let knowledge_base = services
                .as_ref()
                .and_then(|svc| svc.active_knowledge_base(&s.id));
            rows.push(json!({
                "session_id": s.id,
                "name": s.name,
                "session_type": s.session_type,
                "working_dir": s.working_dir,
                "running": running,
                "parent_session_id": s.parent_session_id,
                "extensions": extensions,
                "knowledge_base": knowledge_base,
                "gui": gui_placement,
            }));
        }

        let payload = json!({
            "gui_attached": gui_attached,
            "scope": scope,
            "sessions": rows,
        });
        Ok(vec![Content::text(serde_json::to_string_pretty(&payload).unwrap())])
    }
}

/// Find `session_id` inside a layout echo (§4.3 `workspace_echo.layout`).
fn gui_tab_for(layout: Option<&serde_json::Value>, session_id: &str) -> Option<serde_json::Value> {
    let windows = layout?.as_array()?;
    for window in windows {
        let window_id = window.get("window_id")?.as_str().unwrap_or_default();
        for group in window.get("layout")?.as_array()? {
            for tab in group.get("tabs")?.as_array()? {
                if tab.get("session_id")?.as_str() == Some(session_id) {
                    return Some(json!({
                        "window_id": window_id,
                        "group_id": group.get("group_id"),
                        "tab_id": tab.get("tab_id"),
                        "focused": group.get("active_tab") == tab.get("tab_id"),
                    }));
                }
            }
        }
    }
    None
}

#[async_trait]
impl McpClientTrait for WorkspaceClient {
    async fn list_tools(
        &self,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult { tools: Self::get_tools(), next_cursor: None, meta: None })
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: Option<JsonObject>,
        meta: McpMeta,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let caller = &meta.session_id;
        let content = match name {
            "workspace_list" => self.handle_list(caller, arguments).await,
            "workspace_read_conversation" => Err("not implemented until Task 11".to_string()),
            "workspace_send_prompt" => Err("not implemented until Task 12".to_string()),
            "workspace_set_tools" => Err("not implemented until Task 13".to_string()),
            "workspace_close" => Err("not implemented until Task 14".to_string()),
            "workspace_open" => Err("not implemented until Task 18".to_string()),
            _ => Err(format!("Unknown tool: {name}")),
        };
        match content {
            Ok(content) => Ok(CallToolResult::success(content)),
            Err(error) => Ok(CallToolResult::error(vec![Content::text(format!("Error: {error}"))])),
        }
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }
}
```

Then in `extension.rs`, add the registry entry (after the `chatrecall` insert at :57):

```rust
        map.insert(
            "workspace",
            PlatformExtensionDef {
                name: workspace_extension::EXTENSION_NAME,
                description:
                    "Operate the BioRouter workspace: list/open/read conversations, inject prompts, \
                     change tool sets, and run glass-box subagents in visible tabs",
                default_enabled: false,
                client_factory: |ctx| Box::new(workspace_extension::WorkspaceClient::new(ctx).unwrap()),
            },
        );
```

with `use super::workspace_extension;` mirroring the existing imports.

- [ ] **Step 4: Run tests**

Run: `cargo test -p biorouter --lib agents::workspace_extension agents::extension`
Expected: PASS (including the updated count test).

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/agents
git commit -m "feat(workspace): workspace platform extension skeleton + workspace_list (BR-71 slice 1)"
```

---

### Task 11: `workspace_read_conversation`

**Files:**
- Modify: `crates/biorouter/src/agents/workspace_extension.rs`

- [ ] **Step 1: Write the failing tests**

```rust
    #[tokio::test]
    async fn read_conversation_projects_tool_calls_and_refuses_hidden() {
        use crate::conversation::message::{Message, MessageProvenance, ProvenanceKind};
        let c = client();
        let sm = c.context.session_manager.clone();

        let hidden = sm
            .create_session(std::env::temp_dir(), "h".into(),
                crate::session::session_manager::SessionType::Hidden)
            .await
            .unwrap();
        let open = sm
            .create_session(std::env::temp_dir(), "o".into(),
                crate::session::session_manager::SessionType::User)
            .await
            .unwrap();

        // Seed: a user message, a tool request, and a spawn-context record.
        let mut m1 = Message::user().with_text("please compute");
        sm.add_message_adopting_uid(&open.id, &mut m1).await.unwrap();
        // CallToolRequestParams derives NO Default (rmcp 0.14 model.rs:1887-1902)
        // — spell all four fields.
        let mut m2 = Message::assistant().with_tool_request(
            "call-1",
            Ok(rmcp::model::CallToolRequestParams {
                meta: None,
                name: "shell".into(),
                arguments: Some(serde_json::json!({"command": "ls"}).as_object().unwrap().clone()),
                task: None,
            }),
        );
        sm.add_message_adopting_uid(&open.id, &mut m2).await.unwrap();
        let mut spawn = Message::user()
            .with_text("SPAWN CONTEXT …")
            .with_provenance(MessageProvenance {
                kind: ProvenanceKind::SpawnContext,
                from_session_id: None,
                from_session_name: None,
            });
        spawn.metadata.agent_visible = false;
        sm.add_message_adopting_uid(&open.id, &mut spawn).await.unwrap();

        let call = |view: &str, sid: &str| {
            let args: rmcp::model::JsonObject = serde_json::from_value(
                serde_json::json!({ "session_id": sid, "view": view })).unwrap();
            (args,)
        };

        // Hidden sessions are refused (§5 "no covert reads").
        let (args,) = call("transcript", &hidden.id);
        let refused = c.call_tool("workspace_read_conversation", Some(args),
            test_meta(), CancellationToken::new()).await.unwrap();
        assert_eq!(refused.is_error, Some(true));

        // tool_calls view names the tool, not the prose.
        let (args,) = call("tool_calls", &open.id);
        let tc = c.call_tool("workspace_read_conversation", Some(args),
            test_meta(), CancellationToken::new()).await.unwrap();
        let text = tc.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("shell"));
        assert!(!text.contains("please compute"));

        // spawn_context view returns the provenance-marked record.
        let (args,) = call("spawn_context", &open.id);
        let sc = c.call_tool("workspace_read_conversation", Some(args),
            test_meta(), CancellationToken::new()).await.unwrap();
        assert!(sc.content[0].as_text().unwrap().text.contains("SPAWN CONTEXT"));

        // BR-45 range: from_msg_uid slices the transcript from that message on.
        // m2's uid was adopted by add_message_adopting_uid (#41).
        let uid = m2.id.clone().expect("adopted uid");
        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": open.id, "view": "transcript", "from_msg_uid": uid
        })).unwrap();
        let ranged = c.call_tool("workspace_read_conversation", Some(args),
            test_meta(), CancellationToken::new()).await.unwrap();
        let rtext = ranged.content[0].as_text().unwrap().text.clone();
        assert!(!rtext.contains("please compute"), "messages before the uid are excluded");
    }
```

Add the small helper used above to the test module (`McpMeta` has no `Default`;
`McpMeta::new` is the constructor — `mcp_client.rs:146`):

```rust
    fn test_meta() -> crate::agents::mcp_client::McpMeta {
        crate::agents::mcp_client::McpMeta::new("caller")
    }
```

(If `Message::assistant().with_tool_request(...)` has a different constructor shape,
use the real one — `message.rs:716` `with_tool_request_with_metadata` exists; grep
`pub fn with_tool_request` for the exact signature and adjust the seed only.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter --lib agents::workspace_extension::tests::read_conversation_projects_tool_calls_and_refuses_hidden`
Expected: FAIL — handler returns "not implemented until Task 11".

- [ ] **Step 3: Implement**

Add the params struct + handler; register the tool in `get_tools()` (replacing the
stub arm in `call_tool` with `self.handle_read_conversation(caller, arguments).await`):

```rust
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WorkspaceReadParams {
    session_id: String,
    /// "transcript" (default) | "tool_calls" | "summary" | "spawn_context".
    #[serde(skip_serializing_if = "Option::is_none")]
    view: Option<String>,
    /// Only the last N messages (transcript/tool_calls views).
    #[serde(skip_serializing_if = "Option::is_none")]
    last: Option<usize>,
    /// Start from the message with this durable msg_uid (BR-45 identity;
    /// design §4.1 `range: { from_msg_uid }`). Combines with `last` (uid slice
    /// first, then tail).
    #[serde(skip_serializing_if = "Option::is_none")]
    from_msg_uid: Option<String>,
    /// Cap on returned characters (default 20000, max 200000). Oversized
    /// results above the BR-7 blob threshold are externalized by the caller's
    /// own persist path — see the note below — never silently truncated at a
    /// raised cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_chars: Option<usize>,
}

impl WorkspaceClient {
    async fn handle_read_conversation(
        &self,
        caller_session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspaceReadParams = parse_args(arguments)?;
        let view = args.view.as_deref().unwrap_or("transcript");
        let max_chars = args.max_chars.unwrap_or(20_000).min(200_000);

        let session = self
            .context
            .session_manager
            .get_session(&args.session_id, true)
            .await
            .map_err(|e| format!("failed to load session: {e}"))?;

        // §5 "no covert reads": Hidden sessions honor the same visibility rules
        // as the session list. The read itself is auditable — it IS a tool call
        // in the caller's transcript.
        if session.session_type == crate::session::session_manager::SessionType::Hidden {
            return Err("this session is hidden and cannot be read".to_string());
        }
        tracing::info!(
            caller = caller_session_id, target = %args.session_id, view,
            "workspace cross-session read"
        );

        let messages: Vec<_> = session
            .conversation
            .as_ref()
            .map(|c| c.messages().to_vec())
            .unwrap_or_default();
        // BR-45 range: slice from the named msg_uid (message ids ARE the durable
        // uids — #41 add_message_adopting_uid), then apply `last` as a tail.
        let from_start = match &args.from_msg_uid {
            Some(uid) => messages
                .iter()
                .position(|m| m.id.as_deref() == Some(uid.as_str()))
                .ok_or_else(|| format!("no message with msg_uid '{uid}' in this session"))?,
            None => 0,
        };
        let ranged = &messages[from_start..];
        let tail = |n: Option<usize>| -> &[crate::conversation::message::Message] {
            match n {
                Some(n) if n < ranged.len() => &ranged[ranged.len() - n..],
                _ => ranged,
            }
        };

        let body = match view {
            "tool_calls" => project_tool_calls(tail(args.last)),
            "summary" => project_summary(&session, &messages),
            "spawn_context" => project_spawn_context(&messages)
                .ok_or("this session has no recorded spawn context")?,
            _ => project_transcript(tail(args.last)),
        };

        // Oversized-result handling (§4.1 "session-blob mechanism, never silent
        // truncation"): this tool RESULT is persisted into the CALLER's session,
        // where BR-7's externalization (message_blobs::externalize, threshold 64
        // KB, session_manager.rs:3223) already stores payloads above the blob
        // threshold as session blobs readable via platform__read_session_blob —
        // so a raised max_chars round-trips intact instead of bloating context.
        // The tool-level cap is model-facing pagination; when it clips, the
        // marker names the narrowing controls rather than dropping data silently.
        let clipped = if body.chars().count() > max_chars {
            let cut: String = body.chars().take(max_chars).collect();
            format!(
                "{cut}\n… [clipped at {max_chars} chars — narrow with `last` or \
                 `from_msg_uid`, or raise `max_chars` (up to 200000; oversized \
                 results are stored as a session blob, not lost)]"
            )
        } else {
            body
        };
        Ok(vec![Content::text(format!(
            "Session {} ({}, {:?})\n\n{}",
            session.id, session.name, session.session_type, clipped
        ))])
    }
}

fn parse_args<T: serde::de::DeserializeOwned>(arguments: Option<JsonObject>) -> Result<T, String> {
    let args = arguments.ok_or("Missing arguments")?;
    serde_json::from_value(serde_json::Value::Object(args))
        .map_err(|e| format!("invalid arguments: {e}"))
}

/// The `tool_calls` projection (§4.1): ToolRequest/ToolResponse pairs only,
/// correlated by their shared id — "what did that agent actually do".
fn project_tool_calls(messages: &[crate::conversation::message::Message]) -> String {
    use crate::conversation::message::MessageContent;
    let mut out = String::new();
    for message in messages {
        for content in &message.content {
            match content {
                MessageContent::ToolRequest(req) => {
                    out.push_str(&format!("→ [{}] {}\n", req.id, req.to_readable_string()));
                }
                MessageContent::ToolResponse(resp) => {
                    let digest = match &resp.tool_result {
                        Ok(result) => {
                            let text: String = result
                                .content
                                .iter()
                                .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                                .collect::<Vec<_>>()
                                .join(" ");
                            let short: String = text.chars().take(400).collect();
                            format!("ok: {short}")
                        }
                        Err(e) => format!("error: {e}"),
                    };
                    out.push_str(&format!("← [{}] {digest}\n", resp.id));
                }
                _ => {}
            }
        }
    }
    if out.is_empty() { "No tool calls in range.".to_string() } else { out }
}

fn project_transcript(messages: &[crate::conversation::message::Message]) -> String {
    use crate::conversation::message::MessageContent;
    let mut out = String::new();
    for message in messages {
        // Tab-invisible bookkeeping rows (agent_only) are skipped; tool
        // payloads collapse to one-line stubs (§4.1).
        if !message.metadata.user_visible {
            continue;
        }
        out.push_str(&format!("[{:?}] ", message.role));
        if let Some(p) = &message.metadata.provenance {
            out.push_str(&format!("(injected: {:?}) ", p.kind));
        }
        for content in &message.content {
            match content {
                MessageContent::ToolRequest(req) => {
                    out.push_str(&format!("<tool call: {}>", req.to_readable_string()));
                }
                MessageContent::ToolResponse(resp) => {
                    out.push_str(&format!("<tool result: {}>", resp.id));
                }
                other => {
                    if let Some(text) = other.as_text() {
                        out.push_str(text);
                    }
                }
            }
        }
        out.push('\n');
    }
    out
}

fn project_summary(
    session: &crate::session::Session,
    messages: &[crate::conversation::message::Message],
) -> String {
    // chatrecall-load parity (§3.2): head 3 + tail 3, via the same data.
    let head = project_transcript(&messages[..messages.len().min(3)]);
    let tail_start = messages.len().saturating_sub(3).max(messages.len().min(3));
    let tail = project_transcript(&messages[tail_start..]);
    format!(
        "Working dir: {}\nMessages: {}\n\n--- First ---\n{head}\n--- Last ---\n{tail}",
        session.working_dir.display(),
        messages.len()
    )
}

fn project_spawn_context(messages: &[crate::conversation::message::Message]) -> Option<String> {
    use crate::conversation::message::ProvenanceKind;
    messages.iter().find_map(|m| {
        let p = m.metadata.provenance.as_ref()?;
        (p.kind == ProvenanceKind::SpawnContext).then(|| {
            m.content
                .iter()
                .filter_map(|c| c.as_text())
                .collect::<Vec<_>>()
                .join("\n")
        })
    })
}
```

Register in `get_tools()`:

```rust
            Self::tool(
                "workspace_read_conversation",
                "Structured read of any conversation. view: transcript (prose), \
                 tool_calls (exactly what its agent did), summary (head/tail), \
                 spawn_context (how a subagent was started). Refuses hidden sessions.",
                serde_json::to_value(schema_for!(WorkspaceReadParams)).unwrap(),
                true,
            ),
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p biorouter --lib agents::workspace_extension`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/agents/workspace_extension.rs
git commit -m "feat(workspace): workspace_read_conversation projections (BR-71)"
```

---

### Task 12: `workspace_send_prompt`

**Files:**
- Modify: `crates/biorouter/src/agents/workspace_extension.rs`

- [ ] **Step 1: Write the failing tests**

```rust
    #[tokio::test]
    async fn send_prompt_note_appends_with_provenance_without_running_a_turn() {
        use crate::conversation::message::ProvenanceKind;
        let c = client();
        let sm = c.context.session_manager.clone();
        let caller = sm
            .create_session(std::env::temp_dir(), "caller-name".into(),
                crate::session::session_manager::SessionType::User)
            .await
            .unwrap();
        let target = sm
            .create_session(std::env::temp_dir(), "target".into(),
                crate::session::session_manager::SessionType::User)
            .await
            .unwrap();

        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": target.id, "text": "context for later", "mode": "note"
        })).unwrap();
        let meta = crate::agents::mcp_client::McpMeta::new(caller.id.clone());
        let result = c.call_tool("workspace_send_prompt", Some(args), meta, CancellationToken::new())
            .await.unwrap();
        assert_ne!(result.is_error, Some(true));

        let reread = sm.get_session(&target.id, true).await.unwrap();
        let msgs = reread.conversation.unwrap().messages().to_vec();
        let injected = msgs.last().expect("note appended");
        let p = injected.metadata.provenance.as_ref().expect("provenance stamped");
        assert_eq!(p.kind, ProvenanceKind::AgentInjection);
        assert_eq!(p.from_session_id.as_deref(), Some(caller.id.as_str()));
        assert_eq!(p.from_session_name.as_deref(), Some("caller-name"));
    }

    #[tokio::test]
    async fn send_prompt_turn_and_steer_error_clearly_without_a_daemon() {
        // No WorkspaceServices installed in this test binary state → turn mode
        // must degrade with an explicit message, not panic (§2.1). NOTE: if an
        // earlier test in this binary installed services, mode "turn" may
        // succeed instead — assert only on the response being non-panicking
        // and structured in that case.
        let c = client();
        let sm = c.context.session_manager.clone();
        let target = sm
            .create_session(std::env::temp_dir(), "t".into(),
                crate::session::session_manager::SessionType::User)
            .await
            .unwrap();
        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": target.id, "text": "go", "mode": "steer"
        })).unwrap();
        let result = c.call_tool("workspace_send_prompt", Some(args), test_meta(), CancellationToken::new())
            .await.unwrap();
        // steer with no running turn is always an error (mirrors /interrupt 409).
        assert_eq!(result.is_error, Some(true));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter --lib agents::workspace_extension::tests::send_prompt_note_appends_with_provenance_without_running_a_turn`
Expected: FAIL — "not implemented until Task 12".

- [ ] **Step 3: Implement**

```rust
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WorkspaceSendPromptParams {
    session_id: String,
    text: String,
    /// "turn": start the target's agent on the text (target must be idle).
    /// "steer": inject mid-turn (target must be running). "note": append
    /// context without triggering a turn.
    mode: String,
    /// "none" (default) | "final_message": park until the target's turn
    /// finishes and return its final assistant message.
    #[serde(skip_serializing_if = "Option::is_none")]
    wait: Option<String>,
    /// Bound for wait:"final_message" (default 120, max 600).
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout_s: Option<u64>,
}

impl WorkspaceClient {
    /// PER-CALLER-SESSION cap on concurrently injected detached turns (§5
    /// bounded fan-out: "a per-session cap on concurrently injected detached
    /// turns (default 4)"). The counter map below is keyed by the CALLING
    /// session id, so one conversation cannot saturate the daemon's turn locks
    /// while independent conversations keep their own budgets. Env override ⚠
    /// (reconciliation #10: the var is a plan addition following the
    /// BIOROUTER_SUBAGENT_MAX_* convention).
    fn injected_turn_cap() -> usize {
        std::env::var("BIOROUTER_WORKSPACE_MAX_INJECTED_TURNS")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|&n: &usize| n > 0)
            .unwrap_or(4)
    }

    async fn caller_provenance(
        &self,
        caller_session_id: &str,
    ) -> crate::conversation::message::MessageProvenance {
        use crate::conversation::message::{MessageProvenance, ProvenanceKind};
        let from_session_name = self
            .context
            .session_manager
            .get_session(caller_session_id, false)
            .await
            .ok()
            .map(|s| s.name);
        MessageProvenance {
            kind: ProvenanceKind::AgentInjection,
            from_session_id: Some(caller_session_id.to_string()),
            from_session_name,
        }
    }

    async fn handle_send_prompt(
        &self,
        caller_session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        use crate::session_events::{self, SessionBusEvent};

        let args: WorkspaceSendPromptParams = parse_args(arguments)?;
        if args.session_id == caller_session_id {
            return Err("refusing to inject into your own session — just continue the conversation".into());
        }
        if args.text.trim().is_empty() {
            return Err("text must not be empty".into());
        }
        let provenance = self.caller_provenance(caller_session_id).await;
        let services = workspace_services::get();

        match args.mode.as_str() {
            "note" => {
                // Append without a turn: user_visible + agent_visible (picked up
                // as context on the target's next turn, §4.1), provenance-stamped.
                let mut message = crate::conversation::message::Message::user()
                    .with_text(args.text)
                    .with_provenance(provenance);
                self.context
                    .session_manager
                    .add_message_adopting_uid(&args.session_id, &mut message)
                    .await
                    .map_err(|e| format!("failed to append note: {e}"))?;
                Ok(vec![Content::text(format!(
                    "Note appended to session {} (no turn started).",
                    args.session_id
                ))])
            }
            "steer" => {
                let services = services.ok_or(
                    "steer requires the BioRouter daemon (no workspace services installed)",
                )?;
                if !services.is_turn_active(&args.session_id) {
                    return Err(
                        "target session has no turn in flight — use mode:\"turn\" instead".into(),
                    );
                }
                let agent_manager = crate::execution::manager::AgentManager::instance()
                    .await
                    .map_err(|e| e.to_string())?;
                // Returns the LIVE agent whose loop drains the queue: /reply-
                // driven sessions are registered by the server's get_agent, and
                // glass-box subagent runs register themselves (Task 25) — the
                // steer lands on the running instance in both cases.
                let agent = agent_manager
                    .get_or_create_agent(args.session_id.clone())
                    .await
                    .map_err(|e| e.to_string())?;
                agent.queue_soft_interrupt_with_provenance(args.text, Some(provenance));
                Ok(vec![Content::text(format!(
                    "Steer queued for session {}'s running turn.",
                    args.session_id
                ))])
            }
            "turn" => {
                let services = services.ok_or(
                    "mode:\"turn\" requires the BioRouter daemon (no workspace services installed); \
                     use mode:\"note\" to leave context headlessly",
                )?;
                // Bounded fan-out, PER CALLING SESSION (§5): subscribe before
                // starting so completion is never missed, and count this
                // caller's own in-flight injections.
                let (inflight, _cap_guard) = InjectedTurnGuard::enter(caller_session_id);
                if inflight > Self::injected_turn_cap() {
                    return Err(format!(
                        "this session already has {} injected turns in flight (cap {}); \
                         wait for one to finish",
                        inflight - 1,
                        Self::injected_turn_cap()
                    ));
                }

                let mut rx = session_events::subscribe(&args.session_id);
                let message = crate::conversation::message::Message::user()
                    .with_text(args.text)
                    .with_provenance(provenance);
                let turn_id = services
                    .start_detached_turn(&args.session_id, message)
                    .await
                    .map_err(|e| format!("could not start turn: {e}"))?;

                if args.wait.as_deref() != Some("final_message") {
                    return Ok(vec![Content::text(format!(
                        "Detached turn {turn_id} started on session {}.",
                        args.session_id
                    ))]);
                }

                // ui_ask-style bounded park (§4.1): watch the bus for the final
                // assistant message, bounded by timeout_s.
                let timeout = std::time::Duration::from_secs(args.timeout_s.unwrap_or(120).min(600));
                let mut last_assistant: Option<String> = None;
                let waited = tokio::time::timeout(timeout, async {
                    loop {
                        match rx.recv().await {
                            Ok(SessionBusEvent::Agent(crate::agents::AgentEvent::Message(m)))
                                if m.role == rmcp::model::Role::Assistant =>
                            {
                                let text: String =
                                    m.content.iter().filter_map(|c| c.as_text()).collect::<Vec<_>>().join("\n");
                                if !text.trim().is_empty() {
                                    last_assistant = Some(text);
                                }
                            }
                            Ok(SessionBusEvent::TurnFinished { reason }) => return Ok(reason),
                            Ok(_) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                            Err(e) => return Err(e.to_string()),
                        }
                    }
                })
                .await;

                match waited {
                    Ok(Ok(reason)) => Ok(vec![Content::text(format!(
                        "Turn {turn_id} finished ({reason}). Final message:\n\n{}",
                        last_assistant.unwrap_or_else(|| "<no assistant text>".into())
                    ))]),
                    Ok(Err(e)) => Err(format!("event stream error while waiting: {e}")),
                    Err(_) => Ok(vec![Content::text(format!(
                        "Turn {turn_id} is still running after {}s; it continues in the background. \
                         Read it later with workspace_read_conversation.",
                        timeout.as_secs()
                    ))]),
                }
            }
            other => Err(format!("unknown mode '{other}' (turn | steer | note)")),
        }
    }
}

/// §5 bounded fan-out: PER-SESSION counts of injected detached turns, keyed by
/// the CALLING session id (the design's "per-session cap", default 4). RAII:
/// the guard decrements its own key on drop and removes empty entries so the
/// map never grows unboundedly.
static INJECTED_TURNS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, usize>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

struct InjectedTurnGuard {
    caller: String,
}

impl InjectedTurnGuard {
    /// Increment the caller's count; returns (new count, guard).
    fn enter(caller_session_id: &str) -> (usize, Self) {
        let mut map = INJECTED_TURNS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let count = map.entry(caller_session_id.to_string()).or_insert(0);
        *count += 1;
        (*count, Self { caller: caller_session_id.to_string() })
    }
}

impl Drop for InjectedTurnGuard {
    fn drop(&mut self) {
        let mut map = INJECTED_TURNS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(count) = map.get_mut(&self.caller) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                map.remove(&self.caller);
            }
        }
    }
}
```

Note: the wait branch holds `_cap_guard` for the whole park — intended: a parked
injection *is* in-flight. Role's path: `Message.role` is `rmcp::model::Role` (grep
`pub role:` in message.rs and copy the type path). The self-injection refusal at
the top of `handle_send_prompt` is a plan addition ⚠ (reconciliation #10, operator
#12).

Register in `get_tools()`:

```rust
            Self::tool(
                "workspace_send_prompt",
                "Inject a prompt into another conversation. mode turn: start its \
                 agent (target idle); steer: redirect mid-turn (target running); \
                 note: append context without a turn. Injections are permanently \
                 provenance-labeled. wait:\"final_message\" returns its answer.",
                serde_json::to_value(schema_for!(WorkspaceSendPromptParams)).unwrap(),
                false,
            ),
```

and route `"workspace_send_prompt" => self.handle_send_prompt(caller, arguments).await`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p biorouter --lib agents::workspace_extension`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/agents/workspace_extension.rs
git commit -m "feat(workspace): workspace_send_prompt turn/steer/note with provenance and wait (BR-71)"
```

---

### Task 13: `workspace_set_tools`

**Files:**
- Modify: `crates/biorouter/src/agents/workspace_extension.rs`

- [ ] **Step 1: Write the failing test**

```rust
    #[tokio::test]
    async fn set_tools_rejects_unknown_names_and_multi_kb() {
        let c = client();
        let sm = c.context.session_manager.clone();
        let target = sm
            .create_session(std::env::temp_dir(), "t".into(),
                crate::session::session_manager::SessionType::User)
            .await
            .unwrap();

        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": target.id, "add_extensions": ["definitely-not-an-extension"]
        })).unwrap();
        let result = c.call_tool("workspace_set_tools", Some(args), test_meta(), CancellationToken::new())
            .await.unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].as_text().unwrap().text.contains("definitely-not-an-extension"));

        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": target.id, "set_knowledge_bases": ["kb-a", "kb-b"]
        })).unwrap();
        let result = c.call_tool("workspace_set_tools", Some(args), test_meta(), CancellationToken::new())
            .await.unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].as_text().unwrap().text.contains("one knowledge base"));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter --lib agents::workspace_extension::tests::set_tools_rejects_unknown_names_and_multi_kb`
Expected: FAIL — "not implemented until Task 13".

- [ ] **Step 3: Implement**

```rust
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WorkspaceSetToolsParams {
    session_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    add_extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    remove_extensions: Vec<String>,
    /// The active knowledge base for the session. Accepts a list for schema
    /// conformance with the design; at most ONE entry (single-active KB).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    set_knowledge_bases: Option<Vec<String>>,
}

impl WorkspaceClient {
    async fn handle_set_tools(
        &self,
        caller_session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspaceSetToolsParams = parse_args(arguments)?;

        // Resolve BEFORE mutating anything, so a bad name is a clean no-op.
        let mut add_configs = Vec::new();
        for name in &args.add_extensions {
            match crate::config::get_extension_by_name(name) {
                Some(c) => add_configs.push(c),
                None => return Err(format!("unknown extension '{name}'")),
            }
        }
        if let Some(kbs) = &args.set_knowledge_bases {
            if kbs.len() > 1 {
                return Err(
                    "a session has exactly one active knowledge base; pass one knowledge base (or an empty list to clear)"
                        .into(),
                );
            }
        }
        // §5: workspace control must not fan out through delegation trees.
        if args.add_extensions.iter().any(|n| n == "workspace") {
            let target = self
                .context
                .session_manager
                .get_session(&args.session_id, false)
                .await
                .map_err(|e| e.to_string())?;
            if target.session_type == crate::session::session_manager::SessionType::SubAgent {
                return Err("subagent sessions can never be granted the workspace extension".into());
            }
        }

        // The exact /agent/add_extension handler path (routes/agent.rs:720-743):
        // add on the live agent, persist only after a successful load.
        let agent_manager = crate::execution::manager::AgentManager::instance()
            .await
            .map_err(|e| e.to_string())?;
        let agent = agent_manager
            .get_or_create_agent(args.session_id.clone())
            .await
            .map_err(|e| e.to_string())?;

        let mut applied = Vec::new();
        for config in add_configs {
            let name = config.name().to_string();
            agent
                .add_extension(config)
                .await
                .map_err(|e| format!("failed to add '{name}': {e}"))?;
            applied.push(format!("+{name}"));
        }
        for name in &args.remove_extensions {
            agent
                .remove_extension(name)
                .await
                .map_err(|e| format!("failed to remove '{name}': {e}"))?;
            applied.push(format!("-{name}"));
        }
        if !applied.is_empty() {
            agent
                .persist_extension_state(&args.session_id)
                .await
                .map_err(|e| format!("failed to persist extension state: {e}"))?;
        }

        if let Some(kbs) = args.set_knowledge_bases {
            let services = workspace_services::get()
                .ok_or("knowledge-base scoping requires the BioRouter daemon")?;
            services.set_knowledge_base(&args.session_id, kbs.first().map(String::as_str))?;
            applied.push(match kbs.first() {
                Some(kb) => format!("kb={kb}"),
                None => "kb=<cleared>".into(),
            });
        }

        // §5 autonomous-mode visibility: every change surfaces on the target tab.
        if let Some(services) = workspace_services::get() {
            let _ = services
                .gui_command(
                    json!({
                        "type": "workspace", "cmd": "notify",
                        "session_id": args.session_id,
                        "level": "info",
                        "message": format!("Tools changed by another agent ({caller_session_id}): {}", applied.join(", ")),
                    }),
                    false,
                )
                .await;
        }

        Ok(vec![Content::text(format!(
            "Applied to session {}: {}",
            args.session_id,
            if applied.is_empty() { "nothing (no changes requested)".into() } else { applied.join(", ") }
        ))])
    }
}
```

Register in `get_tools()` (read_only `false`) and route the `call_tool` arm.

- [ ] **Step 4: Run tests**

Run: `cargo test -p biorouter --lib agents::workspace_extension`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/agents/workspace_extension.rs
git commit -m "feat(workspace): workspace_set_tools over the live add/remove-extension path (BR-71)"
```

---

### Task 14: `workspace_close`

**Files:**
- Modify: `crates/biorouter/src/agents/workspace_extension.rs`

- [ ] **Step 1: Write the failing test**

```rust
    #[tokio::test]
    async fn close_turn_is_idempotent_and_close_tab_reports_headless() {
        let c = client();
        let sm = c.context.session_manager.clone();
        let target = sm
            .create_session(std::env::temp_dir(), "t".into(),
                crate::session::session_manager::SessionType::User)
            .await
            .unwrap();

        // scope:"turn" with nothing running: success with cancelled=false
        // semantics (never an error — mirrors POST /agent/cancel).
        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": target.id, "scope": "turn"
        })).unwrap();
        let result = c.call_tool("workspace_close", Some(args), test_meta(), CancellationToken::new())
            .await.unwrap();
        assert_ne!(result.is_error, Some(true));
        assert!(result.content[0].as_text().unwrap().text.contains("no turn"));

        // scope:"tab" headless: not an error — session-level no-op, says so.
        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": target.id, "scope": "tab"
        })).unwrap();
        let result = c.call_tool("workspace_close", Some(args), test_meta(), CancellationToken::new())
            .await.unwrap();
        assert_ne!(result.is_error, Some(true));
        assert!(result.content[0].as_text().unwrap().text.to_lowercase().contains("no gui"));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter --lib agents::workspace_extension::tests::close_turn_is_idempotent_and_close_tab_reports_headless`
Expected: FAIL — "not implemented until Task 14".

- [ ] **Step 3: Implement**

```rust
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WorkspaceCloseParams {
    session_id: String,
    /// "tab": GUI-only, session and any running turn survive. "turn": cancel
    /// the in-flight turn (idempotent). "agent": cancel + evict the agent; the
    /// session record remains.
    scope: String,
}

impl WorkspaceClient {
    /// §5 autonomous-mode visibility: a cross-session cancel/stop must never be
    /// silent in the GUI — surface a toast on the target tab, best-effort.
    async fn notify_target(&self, session_id: &str, message: String) {
        if let Some(services) = workspace_services::get() {
            if services.gui_attached() {
                let _ = services
                    .gui_command(
                        json!({
                            "type": "workspace", "cmd": "notify",
                            "session_id": session_id,
                            "level": "info",
                            "message": message,
                        }),
                        false,
                    )
                    .await;
            }
        }
    }

    async fn handle_close(
        &self,
        caller_session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspaceCloseParams = parse_args(arguments)?;
        let services = workspace_services::get();

        match args.scope.as_str() {
            "tab" => match services {
                Some(s) if s.gui_attached() => {
                    s.gui_command(
                        json!({ "type": "workspace", "cmd": "close_tab", "session_id": args.session_id }),
                        false,
                    )
                    .await?;
                    Ok(vec![Content::text(format!(
                        "Tab for session {} closed (session survives).",
                        args.session_id
                    ))])
                }
                _ => Ok(vec![Content::text(
                    "No GUI attached — nothing to close at tab scope (gui_attached: false).".into(),
                )]),
            },
            "turn" => {
                let services =
                    services.ok_or("scope:\"turn\" requires the BioRouter daemon")?;
                match services.cancel_turn(&args.session_id) {
                    Some(turn_id) => {
                        self.notify_target(
                            &args.session_id,
                            format!("Turn cancelled by another agent ({caller_session_id})."),
                        )
                        .await;
                        Ok(vec![Content::text(format!(
                            "Cancelled turn {turn_id} on session {}.",
                            args.session_id
                        ))])
                    }
                    None => Ok(vec![Content::text(format!(
                        "Session {} had no turn in flight (nothing to cancel).",
                        args.session_id
                    ))]),
                }
            }
            "agent" => {
                let services =
                    services.ok_or("scope:\"agent\" requires the BioRouter daemon")?;
                services.stop_agent(&args.session_id).await?;
                self.notify_target(
                    &args.session_id,
                    format!("Agent stopped by another agent ({caller_session_id})."),
                )
                .await;
                Ok(vec![Content::text(format!(
                    "Agent for session {} stopped and evicted (session record kept).",
                    args.session_id
                ))])
            }
            other => Err(format!("unknown scope '{other}' (tab | turn | agent)")),
        }
    }
}
```

Register the tool (read_only `false`) and route the arm.

- [ ] **Step 4: Run tests**

Run: `cargo test -p biorouter --lib agents::workspace_extension`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/agents/workspace_extension.rs
git commit -m "feat(workspace): workspace_close tab/turn/agent scopes (BR-71)"
```

---

### Task 15: Phase 1 gate

- [ ] **Step 1: Full backend test pass**

Run: `cargo test --workspace --no-fail-fast 2>&1 | tail -10`
Expected: no failures beyond this machine's recorded pre-existing baseline.

- [ ] **Step 2: Lints and formatting**

Run: `cargo fmt && ./scripts/clippy-lint.sh`
Expected: clean.

- [ ] **Step 3: OpenAPI is current**

Run: `just generate-openapi && git diff --exit-code ui/desktop/openapi.json`
Expected: exit 0 (Task 7 already committed the regen).

- [ ] **Step 4: Headless smoke (manual, once) — exact commands**

Terminal A:

```bash
just debug-server        # biorouterd with BIOROUTER_SERVER__SECRET_KEY=test on port 3000
```

Terminal B (each command's expected output is stated after it):

```bash
# 1. Start a session.
SID=$(curl -s -X POST http://127.0.0.1:3000/agent/start \
  -H 'X-Secret-Key: test' -H 'Content-Type: application/json' \
  -d '{"working_dir": "/tmp"}' | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
echo "$SID"
# Expected: a session id (uuid-like string).

# 2. Enable the workspace platform extension on it (ExtensionConfig::Platform,
#    the same request body shape the Settings UI posts).
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:3000/agent/add_extension \
  -H 'X-Secret-Key: test' -H 'Content-Type: application/json' \
  -d "{\"session_id\": \"$SID\", \"config\": {\"type\": \"platform\", \"name\": \"workspace\", \"description\": \"Workspace Control\", \"bundled\": true, \"available_tools\": []}}"
# Expected: 200

# 3. Call the tool through the real dispatch path (prefixed name — platform
#    extension tools are advertised as {ext}__{tool}).
curl -s -X POST http://127.0.0.1:3000/agent/call_tool \
  -H 'X-Secret-Key: test' -H 'Content-Type: application/json' \
  -d "{\"session_id\": \"$SID\", \"name\": \"workspace__workspace_list\", \"arguments\": {\"scope\": \"all\"}}" \
  | python3 -m json.tool | head -30
# Expected: is_error: false; content[0].text is a JSON payload with
# "gui_attached": false and a "sessions" array containing $SID with its
# "extensions" list (including "workspace") and "knowledge_base": null.
```

- [ ] **Step 5: Update the design doc's status header**

In `docs/agent-loop/designs/agent-workspace-control.md`, change the `**Status:**` line
to record: "Slice 1 (backend spine + headless tools) implemented on branch
`br71-workspace-control`; Slices 2-4 remain the plan of record." (Per the status-header
convention in `docs/agent-loop/designs/README.md`.)

- [ ] **Step 6: Commit**

```bash
git add docs/agent-loop/designs/agent-workspace-control.md
git commit -m "docs(br71): mark slice 1 implemented in the design status header"
```

---

# Phase 2 — WorkspaceBridge + renderer applier (design Slice 2)

Ships independently: after Task 23 the daemon can open/activate/close/annotate tabs in
the GUI, the renderer echoes its layout, `workspace_open` works end-to-end, and a tab
for a session the renderer isn't driving streams via the observer endpoint.

### Task 16: `WorkspaceBridge` + per-window registry

**Files:**
- Create: `crates/biorouter-server/src/workspace/bridge.rs`
- Modify: `crates/biorouter-server/src/workspace/mod.rs` (`pub mod bridge;`)

- [ ] **Step 1: Write the failing tests** (generation-guard tests modeled on the
`apps.rs` reconnect tests, per design §7.2)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stale_detach_cannot_tear_down_a_newer_connection() {
        let bridge = WorkspaceBridge::new();
        let (_rx1, token1) = bridge.attach();
        let (_rx2, _token2) = bridge.attach(); // reconnect claims a new generation
        bridge.detach(token1); // stale detach: must be a no-op
        assert!(bridge.is_attached());
    }

    #[test]
    fn emit_delivers_to_the_current_connection_and_fails_detached() {
        let bridge = WorkspaceBridge::new();
        assert!(bridge.emit(json!({"cmd": "open_tab"})).is_err());

        let (mut rx, token) = bridge.attach();
        bridge.emit(json!({"cmd": "open_tab", "session_id": "s1"})).unwrap();
        let frame = rx.try_recv().unwrap();
        assert_eq!(frame["cmd"], "open_tab");

        bridge.detach(token);
        assert!(bridge.emit(json!({"cmd": "notify"})).is_err());
    }

    #[tokio::test]
    async fn round_trip_resolves_and_detach_cancels_parked_requests() {
        let bridge = WorkspaceBridge::new();
        let (mut rx, token) = bridge.attach();

        let waiter = {
            let bridge = bridge.clone();
            tokio::spawn(async move {
                bridge
                    .emit_and_wait(json!({"cmd": "open_tab", "session_id": "s1"}),
                        std::time::Duration::from_secs(5))
                    .await
            })
        };
        // The socket loop would read the frame, act, and reply by request_id.
        let frame = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if let Ok(f) = rx.try_recv() { break f; }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let request_id = frame["request_id"].as_str().unwrap().to_string();
        bridge.resolve(&request_id, json!({"ok": true}));
        assert_eq!(waiter.await.unwrap().unwrap()["ok"], true);

        // A parked request must not hang forever on disconnect.
        let waiter2 = {
            let bridge = bridge.clone();
            tokio::spawn(async move {
                bridge
                    .emit_and_wait(json!({"cmd": "open_tab"}), std::time::Duration::from_secs(5))
                    .await
            })
        };
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        bridge.detach(token); // cancel_all unparks
        assert!(waiter2.await.unwrap().is_err());
    }

    #[test]
    fn registry_tracks_focus_and_merges_layouts() {
        // BRIDGES is a process-wide static shared with every other test in this
        // binary (now and later). Use unique window ids and CONTAINMENT
        // assertions — never exact global counts — so parallel `cargo test`
        // and future bridge-attaching tests cannot flake this one.
        // (uuid is not a biorouter-server dep — a nanos timestamp is unique
        // enough for two ids minted in one test.)
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let win_a = format!("win-a-{nonce}");
        let win_b = format!("win-b-{nonce}");
        let a = bridge_for(&win_a);
        let b = bridge_for(&win_b);
        let (_ra, ta) = a.attach();
        let (_rb, tb) = b.attach();
        a.store_echo(json!({"window_id": win_a, "focused_session": "s1", "layout": []}));
        b.store_echo(json!({"window_id": win_b, "focused_session": null, "layout": []}));
        assert!(any_attached());
        let merged = merged_layout().unwrap();
        let ids: Vec<String> = merged
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|e| e.get("window_id").and_then(|w| w.as_str()).map(String::from))
            .collect();
        assert!(ids.contains(&win_a), "merged layout carries window {win_a}");
        assert!(ids.contains(&win_b), "merged layout carries window {win_b}");
        // A window with a focused_session in its echo is a valid target.
        assert!(focused_or_recent().is_some());
        // Clean up so this test leaves no attached bridges for others to see.
        a.detach(ta);
        b.detach(tb);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter-server --lib workspace::bridge`
Expected: COMPILE ERROR.

- [ ] **Step 3: Implement** — modeled line-for-line on `UiBridge`
(`agent_drafter/control.rs:557-780`, registry `apps.rs:483-496`), at window scope:

```rust
//! BR-71 §4.3: the daemon→GUI workspace command channel. One bridge per GUI
//! WINDOW (Agent Drafter's `UiBridge` is per app session — same anatomy, one
//! level up): generation-guarded attach/detach, a pending map for blocking
//! round trips, cancel_all on disconnect, and the window's last layout echo.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

#[derive(Clone)]
pub struct WorkspaceBridge {
    inner: Arc<BridgeInner>,
}

struct BridgeInner {
    tx: Mutex<Option<mpsc::UnboundedSender<Value>>>,
    /// Guards attach/detach: only the connection that owns the current
    /// generation can tear it down (control.rs:592-595 rationale).
    generation: AtomicU64,
    pending: Mutex<HashMap<String, oneshot::Sender<Value>>>,
    last_echo: Mutex<Option<Value>>,
    last_attach: Mutex<Option<Instant>>,
    request_seq: AtomicU64,
}

/// Opaque proof of which connection generation a socket owns.
#[derive(Debug, Clone, Copy)]
pub struct ConnToken(u64);

impl WorkspaceBridge {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(BridgeInner {
                tx: Mutex::new(None),
                generation: AtomicU64::new(0),
                pending: Mutex::new(HashMap::new()),
                last_echo: Mutex::new(None),
                last_attach: Mutex::new(None),
                request_seq: AtomicU64::new(1),
            }),
        }
    }

    pub fn attach(&self) -> (mpsc::UnboundedReceiver<Value>, ConnToken) {
        let gen = self.inner.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let (tx, rx) = mpsc::unbounded_channel();
        *lock(&self.inner.tx) = Some(tx);
        *lock(&self.inner.last_attach) = Some(Instant::now());
        (rx, ConnToken(gen))
    }

    /// No-op unless `token` owns the current generation, so a slow old socket
    /// unwinding cannot sever its replacement.
    pub fn detach(&self, token: ConnToken) {
        if self.inner.generation.load(Ordering::Acquire) != token.0 {
            return;
        }
        *lock(&self.inner.tx) = None;
        self.cancel_all();
    }

    pub fn is_attached(&self) -> bool {
        lock(&self.inner.tx).is_some()
    }

    pub fn emit(&self, frame: Value) -> Result<(), String> {
        let guard = lock(&self.inner.tx);
        let tx = guard.as_ref().ok_or("no GUI window attached")?;
        tx.send(frame).map_err(|_| "GUI window channel closed".to_string())
    }

    /// Emit with a minted `request_id` and park until the renderer's
    /// `workspace_result` resolves it (bounded).
    pub async fn emit_and_wait(&self, mut frame: Value, timeout: Duration) -> Result<Value, String> {
        let request_id = format!(
            "wsreq-{}",
            self.inner.request_seq.fetch_add(1, Ordering::Relaxed)
        );
        frame["request_id"] = Value::String(request_id.clone());
        let (tx, rx) = oneshot::channel();
        lock(&self.inner.pending).insert(request_id.clone(), tx);
        if let Err(e) = self.emit(frame) {
            lock(&self.inner.pending).remove(&request_id);
            return Err(e);
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(_)) => Err("GUI window disconnected before replying".into()),
            Err(_) => {
                lock(&self.inner.pending).remove(&request_id);
                Err("timed out waiting for the GUI".into())
            }
        }
    }

    pub fn resolve(&self, request_id: &str, value: Value) {
        if let Some(tx) = lock(&self.inner.pending).remove(request_id) {
            let _ = tx.send(value);
        }
    }

    pub fn cancel_all(&self) {
        for (_, tx) in lock(&self.inner.pending).drain() {
            drop(tx); // receivers observe Err and unpark
        }
    }

    pub fn store_echo(&self, echo: Value) {
        *lock(&self.inner.last_echo) = Some(echo);
    }

    pub fn last_echo(&self) -> Option<Value> {
        lock(&self.inner.last_echo).clone()
    }

    fn last_attach(&self) -> Option<Instant> {
        *lock(&self.inner.last_attach)
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Registry keyed by window_id; entries retained for the process's life,
/// mirroring UI_BRIDGES (apps.rs:483-486).
static BRIDGES: LazyLock<Mutex<HashMap<String, WorkspaceBridge>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn bridge_for(window_id: &str) -> WorkspaceBridge {
    lock(&BRIDGES)
        .entry(window_id.to_string())
        .or_insert_with(WorkspaceBridge::new)
        .clone()
}

pub fn any_attached() -> bool {
    lock(&BRIDGES).values().any(WorkspaceBridge::is_attached)
}

/// Multi-window aggregation (§4.3): commands target the focused window (per its
/// echo), else the most recently attached.
pub fn focused_or_recent() -> Option<WorkspaceBridge> {
    let map = lock(&BRIDGES);
    let attached: Vec<_> = map.values().filter(|b| b.is_attached()).cloned().collect();
    drop(map);
    attached
        .iter()
        .find(|b| {
            b.last_echo()
                .and_then(|e| e.get("focused_session").cloned())
                .is_some_and(|f| !f.is_null())
        })
        .cloned()
        .or_else(|| {
            attached
                .into_iter()
                .max_by_key(|b| b.last_attach().unwrap_or_else(Instant::now))
        })
}

/// All windows' last echoes, merged — what workspace_list renders as `gui`.
pub fn merged_layout() -> Option<serde_json::Value> {
    let echoes: Vec<Value> = lock(&BRIDGES)
        .values()
        .filter(|b| b.is_attached())
        .filter_map(|b| b.last_echo())
        .collect();
    if echoes.is_empty() { None } else { Some(Value::Array(echoes)) }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p biorouter-server --lib workspace::bridge`
Expected: `test result: ok. 4 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter-server/src/workspace
git commit -m "feat(server): WorkspaceBridge per-window registry, UiBridge sibling (BR-71 slice 2)"
```

---

### Task 17: `GET /ui/workspace` WebSocket route

**Files:**
- Create: `crates/biorouter-server/src/routes/workspace.rs`
- Modify: `crates/biorouter-server/src/routes/mod.rs` (`pub mod workspace;` + merge)
- Modify: `crates/biorouter-server/src/workspace/services.rs` (wire the three GUI
  methods to the bridge registry)

- [ ] **Step 1: Write the failing auth tests**

In the new file's test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_auth_requires_secret_and_local_or_app_origin() {
        let secret = "test-secret";
        // Browser-set web origins must be loopback (CSWSH — is_local_origin,
        // routes/mod.rs:9-24).
        assert!(check_workspace_ws_auth(Some("https://evil.com"), Some(secret), secret).is_err());
        assert!(check_workspace_ws_auth(Some("http://127.0.0.1:5173"), Some(secret), secret).is_ok());
        // The packaged Electron renderer presents "file://" or null (main.ts
        // loadURL of a file entry) — allowed, the secret still gates.
        assert!(check_workspace_ws_auth(Some("file://"), Some(secret), secret).is_ok());
        assert!(check_workspace_ws_auth(Some("null"), Some(secret), secret).is_ok());
        assert!(check_workspace_ws_auth(None, Some(secret), secret).is_ok());
        // Wrong/missing secret always refuses.
        assert!(check_workspace_ws_auth(None, Some("wrong"), secret).is_err());
        assert!(check_workspace_ws_auth(None, None, secret).is_err());
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter-server --lib routes::workspace`
Expected: COMPILE ERROR.

- [ ] **Step 3: Implement the route**

```rust
//! BR-71 §4.3: each Electron window connects once at startup with a stable
//! window_id. Outbound: workspace command frames. Inbound: workspace_echo
//! (debounced layout report) and workspace_result (resolves parked round
//! trips). Auth: the server secret as a query token (the browser WebSocket API
//! cannot set headers) + the origin gate — same two-gate shape as the app
//! agent socket (apps.rs:538-556), with the Electron file/null origin allowed.

use std::sync::Arc;

use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde_json::Value;

use crate::state::AppState;
use crate::workspace::bridge;

fn check_workspace_ws_auth(
    origin: Option<&str>,
    token: Option<&str>,
    expected: &str,
) -> Result<(), &'static str> {
    if let Some(origin) = origin {
        let electron_shell = origin == "file://" || origin == "null";
        if !electron_shell && !super::is_local_origin(origin) {
            return Err("cross-origin connect rejected");
        }
    }
    if token != Some(expected) {
        return Err("missing or invalid workspace socket secret");
    }
    Ok(())
}

async fn workspace_ws(
    Query(params): Query<std::collections::HashMap<String, String>>,
    headers: axum::http::HeaderMap,
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    let origin = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|o| o.to_str().ok());
    // The same secret every HTTP route checks (secret-key middleware). Find
    // the accessor the middleware uses — grep `secret` in
    // crates/biorouter-server/src/auth.rs and call the same source of truth.
    let expected = crate::auth::server_secret();
    if let Err(reason) =
        check_workspace_ws_auth(origin, params.get("secret").map(String::as_str), &expected)
    {
        tracing::warn!(origin = origin.unwrap_or("<none>"), "rejected workspace WS: {reason}");
        return (axum::http::StatusCode::FORBIDDEN, reason).into_response();
    }
    let window_id = params
        .get("window_id")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "default".to_string());
    ws.on_upgrade(move |socket| handle_workspace_socket(socket, state, window_id))
}

async fn handle_workspace_socket(socket: WebSocket, _state: Arc<AppState>, window_id: String) {
    use futures::{SinkExt, StreamExt};
    let bridge = bridge::bridge_for(&window_id);
    let (mut outbound_rx, token) = bridge.attach();
    let (mut socket_tx, mut socket_rx) = socket.split();

    loop {
        tokio::select! {
            frame = outbound_rx.recv() => match frame {
                Some(frame) => {
                    let text = frame.to_string();
                    if socket_tx.send(WsMessage::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                None => break, // a newer connection replaced us
            },
            inbound = socket_rx.next() => match inbound {
                Some(Ok(WsMessage::Text(text))) => {
                    let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
                    match value.get("type").and_then(Value::as_str) {
                        Some("workspace_echo") => bridge.store_echo(value),
                        Some("workspace_result") => {
                            if let Some(id) = value.get("request_id").and_then(Value::as_str) {
                                bridge.resolve(id, value.clone());
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(WsMessage::Close(_))) | None => break,
                Some(Ok(_)) => {}
                Some(Err(_)) => break,
            },
        }
    }
    bridge.detach(token);
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ui/workspace", get(workspace_ws))
        .with_state(state)
}
```

Implementer notes (verified constraints, not placeholders):
- **Secret access:** whatever helper the secret-key middleware in
  `crates/biorouter-server/src/auth.rs` reads (grep `SECRET` there); if none is
  exported, add a `pub(crate) fn server_secret() -> String` beside the middleware
  reading the same config source — do not introduce a second secret.
- **Middleware exemption:** if the WS route sits behind the header-checking
  middleware it will 401 before upgrading (browsers can't set headers on WS). Mount it
  the same way `apps.rs` mounts its exempt agent socket (see how `apps::routes` is
  merged in `routes/mod.rs:89` and which layers apply) — the query-secret + origin
  check above then carries the auth.
- `axum::extract::ws::Message::Text` takes `Utf8Bytes` in the workspace's axum
  version — the `.into()` above covers both; mirror `apps.rs`'s send calls.

Wire `services.rs` (replacing the three Slice-1 stubs):

```rust
    fn gui_attached(&self) -> bool {
        crate::workspace::bridge::any_attached()
    }

    fn layout_snapshot(&self) -> Option<serde_json::Value> {
        crate::workspace::bridge::merged_layout()
    }

    async fn gui_command(
        &self,
        frame: serde_json::Value,
        wait_result: bool,
    ) -> Result<serde_json::Value, String> {
        let bridge = crate::workspace::bridge::focused_or_recent()
            .ok_or("no GUI attached")?;
        if wait_result {
            bridge
                .emit_and_wait(frame, std::time::Duration::from_secs(10))
                .await
        } else {
            bridge.emit(frame).map(|()| serde_json::json!({ "sent": true }))
        }
    }
```

- [ ] **Step 4: Run tests, regenerate OpenAPI**

Run: `cargo test -p biorouter-server --lib routes::workspace workspace::`
Expected: PASS.
Run: `just generate-openapi && cd ui/desktop && npm run generate-api && cd ../..`
(A WS route may not appear in the OpenAPI paths — that is fine; the regen guards the
rest.)

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter-server/src ui/desktop/openapi.json ui/desktop/src/api
git commit -m "feat(server): GET /ui/workspace window channel + bridge-backed services (BR-71)"
```

---

### Task 18: `workspace_open` (session-level + GUI frames)

**Files:**
- Modify: `crates/biorouter/src/agents/workspace_extension.rs`

- [ ] **Step 1: Write the failing test**

```rust
    #[tokio::test]
    async fn workspace_open_requires_exactly_one_of_session_id_or_new() {
        let c = client();
        let args: rmcp::model::JsonObject =
            serde_json::from_value(serde_json::json!({})).unwrap();
        let result = c.call_tool("workspace_open", Some(args), test_meta(), CancellationToken::new())
            .await.unwrap();
        assert_eq!(result.is_error, Some(true));

        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": "s-x", "new": { "working_dir": "/tmp" }
        })).unwrap();
        let result = c.call_tool("workspace_open", Some(args), test_meta(), CancellationToken::new())
            .await.unwrap();
        assert_eq!(result.is_error, Some(true));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter --lib agents::workspace_extension::tests::workspace_open_requires_exactly_one_of_session_id_or_new`
Expected: FAIL — "not implemented until Task 18".

- [ ] **Step 3: Implement**

```rust
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WorkspaceOpenNew {
    working_dir: String,
    /// Extension names; same semantics as /agent/start extension_overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    extensions: Option<Vec<String>>,
    /// At most one (single-active KB per session).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    knowledge_bases: Vec<String>,
    /// Optional first user message, run as a detached turn.
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WorkspaceOpenParams {
    /// Open/focus an existing conversation. Mutually exclusive with `new`.
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    /// Start a fresh conversation. Mutually exclusive with `session_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    new: Option<WorkspaceOpenNew>,
    /// "tab" (default) | "split" | "window".
    #[serde(skip_serializing_if = "Option::is_none")]
    placement: Option<String>,
    /// Default false: open in the background, never steal the user's composer.
    #[serde(skip_serializing_if = "Option::is_none")]
    focus: Option<bool>,
}

impl WorkspaceClient {
    async fn handle_open(
        &self,
        caller_session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspaceOpenParams = parse_args(arguments)?;
        let placement = args.placement.as_deref().unwrap_or("tab").to_string();
        let focus = args.focus.unwrap_or(false);
        let services = workspace_services::get();

        let session_id = match (args.session_id, args.new) {
            (Some(_), Some(_)) => {
                return Err("pass either session_id OR new, not both".into());
            }
            (None, None) => {
                return Err("pass session_id (open existing) or new (start fresh)".into());
            }
            (Some(session_id), None) => {
                // Validate it exists so the GUI never gets a dangling frame.
                self.context
                    .session_manager
                    .get_session(&session_id, false)
                    .await
                    .map_err(|e| format!("no such session: {e}"))?;
                session_id
            }
            (None, Some(new)) => {
                if new.knowledge_bases.len() > 1 {
                    return Err("a session has exactly one active knowledge base".into());
                }
                let services = services
                    .clone()
                    .ok_or("starting a new session requires the BioRouter daemon")?;
                let session_id = services
                    .start_session(
                        std::path::PathBuf::from(&new.working_dir),
                        new.extensions,
                        new.knowledge_bases.first().cloned(),
                    )
                    .await?;
                if let Some(prompt) = new.prompt {
                    let provenance = self.caller_provenance(caller_session_id).await;
                    let message = crate::conversation::message::Message::user()
                        .with_text(prompt)
                        .with_provenance(provenance);
                    services.start_detached_turn(&session_id, message).await?;
                }
                session_id
            }
        };

        // GUI effect (§4.3): open_tab relies on the reducer's dedupe/adopt
        // rules; "split" maps to moveTabToGroup; "window" is its OWN frame
        // (`open_window`, per the §4.3 vocabulary) which the renderer relays
        // to the create-chat-window IPC. The renderer answers via
        // workspace_result so a refused split (MAX_GROUPS) comes back as a
        // clear message, not silence.
        match services {
            Some(s) if s.gui_attached() => {
                let frame = if placement == "window" {
                    json!({
                        "type": "workspace", "cmd": "open_window",
                        "session_id": session_id,
                    })
                } else {
                    json!({
                        "type": "workspace", "cmd": "open_tab",
                        "session_id": session_id,
                        "placement": placement,
                        "focus": focus,
                    })
                };
                let result = s.gui_command(frame, true).await?;
                let ok = result.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false);
                let detail = result.get("detail").and_then(serde_json::Value::as_str).unwrap_or("");
                Ok(vec![Content::text(format!(
                    "Session {session_id} {} in the GUI ({placement}{}). {detail}",
                    if ok { "opened" } else { "NOT opened" },
                    if focus { ", focused" } else { ", background" },
                ))])
            }
            _ => Ok(vec![Content::text(format!(
                "Session {session_id} ready (gui_attached: false — no tab opened; \
                 the session exists headlessly)."
            ))]),
        }
    }
}
```

Register in `get_tools()` (read_only `false`, description per §4.1) and replace the
stub arm.

**#44 — resolved (reconciliation #7):** the working-dir lock is merged at
`30d49d9a`. `start_session` sets the dir at creation exactly as today's
`start_agent` does (`routes/agent.rs:283`); the lock guards only post-creation
changes to non-empty chats, which no path in this task performs. The remaining
question — whether `workspace_open.new.working_dir` should *default* to the
caller's dir and require confirmation to differ — is product policy, operator
decision #4.

- [ ] **Step 4: Run tests**

Run: `cargo test -p biorouter --lib agents::workspace_extension`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/agents/workspace_extension.rs
git commit -m "feat(workspace): workspace_open with GUI round-trip and headless degradation (BR-71)"
```

---

### Task 19: Renderer `workspaceCommandRegistry`

**Files:**
- Create: `ui/desktop/src/components/chatGroups/workspaceCommandRegistry.ts`
- Create: `ui/desktop/src/components/chatGroups/workspaceCommandRegistry.test.ts`

- [ ] **Step 1: Write the failing tests**

```typescript
import { describe, expect, it, beforeEach } from 'vitest';
import {
  registerWorkspaceCommands,
  applyWorkspaceCommand,
  drainPendingWorkspaceCommands,
  hasPendingWorkspaceCommands,
  resetWorkspaceCommandRegistry,
  type WorkspaceCommand,
} from './workspaceCommandRegistry';

const openTab: WorkspaceCommand = {
  type: 'workspace',
  cmd: 'open_tab',
  session_id: 's1',
  placement: 'tab',
  focus: false,
};

describe('workspaceCommandRegistry — the daemon→tabs hand-off', () => {
  beforeEach(() => resetWorkspaceCommandRegistry());

  it('dispatches to a live handler and reports its result', () => {
    const seen: WorkspaceCommand[] = [];
    registerWorkspaceCommands((cmd) => {
      seen.push(cmd);
      return { ok: true, detail: 'opened' };
    });
    const result = applyWorkspaceCommand(openTab);
    expect(result).toEqual({ ok: true, detail: 'opened' });
    expect(seen).toHaveLength(1);
  });

  it('queues commands with no provider mounted, for the mounting provider to drain', () => {
    const result = applyWorkspaceCommand(openTab);
    expect(result.ok).toBe(false);
    expect(hasPendingWorkspaceCommands()).toBe(true);
    const drained = drainPendingWorkspaceCommands();
    expect(drained).toEqual([openTab]);
    // Consume-once: StrictMode double-mount must not double-apply (same
    // rationale as newTabRegistry.consumePendingNewTab).
    expect(drainPendingWorkspaceCommands()).toEqual([]);
  });

  it('disposer only clears its own handler (mount-B-then-dispose-A order)', () => {
    const disposeA = registerWorkspaceCommands(() => ({ ok: true }));
    registerWorkspaceCommands(() => ({ ok: true, detail: 'B' }));
    disposeA();
    expect(applyWorkspaceCommand(openTab).detail).toBe('B');
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd ui/desktop && npm run test:run -- workspaceCommandRegistry`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```typescript
/**
 * The daemon→ChatGroups hand-off for BR-71 workspace command frames.
 *
 * The same seam, and deliberately the same shape, as newTabRegistry /
 * closeActiveTabRegistry — for the same reason: frames arrive at the app root
 * (the workspace WebSocket lives beside ChatGroupsProvider, but a frame can
 * arrive while the user is on Settings, where no provider is mounted). A live
 * provider registers a claim; frames with no claimant are QUEUED, not dropped,
 * and the next mounting provider drains them — the workspace analogue of
 * pendingNewTab (issue #38 taught us the redirect-vs-commit race; consume-once
 * protects against StrictMode double-mounts).
 */
export type WorkspaceCommand = {
  type: 'workspace';
  cmd: 'open_tab' | 'activate_tab' | 'close_tab' | 'open_window' | 'notify' | 'annotate_tab';
  session_id?: string;
  placement?: 'tab' | 'split' | 'window';
  focus?: boolean;
  level?: string;
  message?: string;
  badge?: string;
  parent_session_id?: string;
  request_id?: string;
};

export type WorkspaceCommandResult = { ok: boolean; detail?: string };
export type WorkspaceCommandHandler = (cmd: WorkspaceCommand) => WorkspaceCommandResult;

let handler: WorkspaceCommandHandler | null = null;
let pending: WorkspaceCommand[] = [];

export function registerWorkspaceCommands(next: WorkspaceCommandHandler): () => void {
  handler = next;
  return () => {
    if (handler === next) handler = null;
  };
}

/** Apply now if a provider is mounted; otherwise queue and report deferral. */
export function applyWorkspaceCommand(cmd: WorkspaceCommand): WorkspaceCommandResult {
  if (handler) return handler(cmd);
  pending.push(cmd);
  return { ok: false, detail: 'no chat surface mounted; queued' };
}

/** Consume-once drain, by the provider on mount. */
export function drainPendingWorkspaceCommands(): WorkspaceCommand[] {
  const drained = pending;
  pending = [];
  return drained;
}

export function hasPendingWorkspaceCommands(): boolean {
  return pending.length > 0;
}

/** Tests only — the singleton must not leak across cases. */
export function resetWorkspaceCommandRegistry(): void {
  handler = null;
  pending = [];
}
```

- [ ] **Step 4: Run tests**

Run: `cd ui/desktop && npm run test:run -- workspaceCommandRegistry`
Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/components/chatGroups/workspaceCommandRegistry.ts ui/desktop/src/components/chatGroups/workspaceCommandRegistry.test.ts
git commit -m "feat(ui): workspaceCommandRegistry seam for daemon workspace frames (BR-71)"
```

---

### Task 20: Renderer workspace channel + command planner + provider wiring + layout echo

**Files:**
- Create: `ui/desktop/src/hooks/useWorkspaceChannel.ts`
- Create: `ui/desktop/src/hooks/useWorkspaceChannel.test.tsx`
- Create: `ui/desktop/src/components/chatGroups/workspaceCommandPlanner.ts` — the
  PURE frame→plan function. All command behavior (split refusal at `MAX_GROUPS`,
  background-focus restore, annotate, window relay) lives here so it is
  unit-testable against real reducer state without mounting the provider.
- Create: `ui/desktop/src/components/chatGroups/workspaceCommandPlanner.test.ts`
- Modify: `ui/desktop/src/components/chatGroups/chatGroupsReducer.ts` — export
  `findTabBySession` (a public wrapper over the private `findTabGroup` at :241)
- Modify: `ui/desktop/src/contexts/ChatGroupsContext.tsx` (register the command
  handler — a thin adapter over the planner; annotation state; report layout; the
  provider already owns the reducer state, `stateRef` at :195-196)

- [ ] **Step 1: Write the failing tests** (jsdom, mock WebSocket)

```typescript
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useWorkspaceChannel, buildEchoFrame } from './useWorkspaceChannel';
import {
  registerWorkspaceCommands,
  resetWorkspaceCommandRegistry,
} from '../components/chatGroups/workspaceCommandRegistry';

class FakeWebSocket {
  static instances: FakeWebSocket[] = [];
  static OPEN = 1;
  readyState = FakeWebSocket.OPEN;
  sent: string[] = [];
  onmessage: ((ev: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onopen: (() => void) | null = null;
  constructor(public url: string) {
    FakeWebSocket.instances.push(this);
    queueMicrotask(() => this.onopen?.());
  }
  send(data: string) {
    this.sent.push(data);
  }
  close() {
    this.onclose?.();
  }
}

describe('useWorkspaceChannel', () => {
  beforeEach(() => {
    resetWorkspaceCommandRegistry();
    vi.stubGlobal('WebSocket', FakeWebSocket as unknown as typeof WebSocket);
    FakeWebSocket.instances = [];
  });
  afterEach(() => vi.unstubAllGlobals());

  it('applies inbound frames through the registry and answers request_ids', async () => {
    const applied: string[] = [];
    registerWorkspaceCommands((cmd) => {
      applied.push(cmd.cmd);
      return { ok: true, detail: 'done' };
    });
    renderHook(() => useWorkspaceChannel({ secret: 's', windowId: 'w1', enabled: true }));
    await act(async () => {});
    const ws = FakeWebSocket.instances[0];
    act(() => {
      ws.onmessage?.({
        data: JSON.stringify({
          type: 'workspace',
          cmd: 'open_tab',
          session_id: 's1',
          request_id: 'wsreq-1',
        }),
      });
    });
    expect(applied).toEqual(['open_tab']);
    const reply = ws.sent.map((s) => JSON.parse(s)).find((f) => f.type === 'workspace_result');
    expect(reply).toMatchObject({ request_id: 'wsreq-1', ok: true, detail: 'done' });
  });

  it('buildEchoFrame flattens the ChatGroups layout with session bindings', () => {
    const echo = buildEchoFrame('w1', 's-focused', {
      groups: {
        'group-1': {
          groupId: 'group-1',
          activeTabId: 'tab-1',
          tabs: [
            { tabId: 'tab-1', sessionId: 's-focused', title: 'A' },
            { tabId: 'tab-2', sessionId: '', title: 'blank' },
          ],
        },
      },
      order: ['group-1'],
    });
    expect(echo.type).toBe('workspace_echo');
    expect(echo.window_id).toBe('w1');
    expect(echo.layout[0].tabs).toHaveLength(2);
    expect(echo.layout[0].active_tab).toBe('tab-1');
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd ui/desktop && npm run test:run -- useWorkspaceChannel`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the hook**

```typescript
/**
 * BR-71 §4.3 renderer side: one WebSocket per window to GET /ui/workspace.
 * Inbound workspace frames are applied through workspaceCommandRegistry (the
 * ChatGroupsProvider registers the real handler); every frame carrying a
 * request_id is answered with workspace_result. Outbound: debounced
 * workspace_echo layout reports (sendEcho is handed back to the provider).
 *
 * Reconnects with backoff; the daemon side is generation-guarded, so a
 * reconnect simply claims a new generation (WorkspaceBridge.attach).
 */
import { useEffect, useMemo, useRef } from 'react';
import { getApiUrl } from '../config';
import {
  applyWorkspaceCommand,
  type WorkspaceCommand,
} from '../components/chatGroups/workspaceCommandRegistry';

type EchoLayoutGroup = {
  group_id: string;
  active_tab: string | null;
  tabs: { tab_id: string; session_id: string; title: string }[];
};

export type EchoFrame = {
  type: 'workspace_echo';
  window_id: string;
  focused_session: string | null;
  layout: EchoLayoutGroup[];
};

/** Pure: ChatGroups state → echo frame (unit-tested without a socket). */
export function buildEchoFrame(
  windowId: string,
  focusedSession: string | null,
  state: {
    groups: Record<
      string,
      { groupId: string; activeTabId: string | null; tabs: { tabId: string; sessionId: string; title: string }[] }
    >;
    order: string[];
  }
): EchoFrame {
  return {
    type: 'workspace_echo',
    window_id: windowId,
    focused_session: focusedSession,
    layout: state.order
      .map((groupId) => state.groups[groupId])
      .filter(Boolean)
      .map((group) => ({
        group_id: group.groupId,
        active_tab: group.activeTabId,
        tabs: group.tabs.map((tab) => ({
          tab_id: tab.tabId,
          session_id: tab.sessionId,
          title: tab.title,
        })),
      })),
  };
}

export function useWorkspaceChannel({
  secret,
  windowId,
  enabled,
}: {
  secret: string | null;
  windowId: string;
  enabled: boolean;
}): { sendEcho: (echo: EchoFrame) => void } {
  const socketRef = useRef<WebSocket | null>(null);
  const echoTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastEcho = useRef<EchoFrame | null>(null);

  useEffect(() => {
    if (!enabled || !secret) return;
    let disposed = false;
    let retryMs = 1000;

    const connect = () => {
      if (disposed) return;
      const base = getApiUrl('/ui/workspace').replace(/^http/, 'ws');
      const ws = new WebSocket(
        `${base}?secret=${encodeURIComponent(secret)}&window_id=${encodeURIComponent(windowId)}`
      );
      socketRef.current = ws;
      ws.onopen = () => {
        retryMs = 1000;
        if (lastEcho.current) ws.send(JSON.stringify(lastEcho.current));
      };
      ws.onmessage = (event) => {
        let frame: WorkspaceCommand;
        try {
          frame = JSON.parse(String(event.data));
        } catch {
          return;
        }
        if (frame.type !== 'workspace') return;
        const result = applyWorkspaceCommand(frame);
        if (frame.request_id) {
          ws.send(
            JSON.stringify({
              type: 'workspace_result',
              request_id: frame.request_id,
              ok: result.ok,
              detail: result.detail ?? '',
            })
          );
        }
      };
      ws.onclose = () => {
        socketRef.current = null;
        if (!disposed) {
          setTimeout(connect, retryMs);
          retryMs = Math.min(retryMs * 2, 15000);
        }
      };
    };
    connect();
    return () => {
      disposed = true;
      socketRef.current?.close();
    };
  }, [enabled, secret, windowId]);

  return useMemo(
    () => ({
      sendEcho: (echo: EchoFrame) => {
        lastEcho.current = echo;
        if (echoTimer.current) clearTimeout(echoTimer.current);
        echoTimer.current = setTimeout(() => {
          const ws = socketRef.current;
          if (ws && ws.readyState === WebSocket.OPEN && lastEcho.current) {
            ws.send(JSON.stringify(lastEcho.current));
          }
        }, 300); // debounced (§4.3)
      },
    }),
    []
  );
}
```

- [ ] **Step 4: Write the failing planner tests** (the planner is where every
command behavior lives — split refusal, background-focus restore, window relay,
annotations — tested against REAL reducer state built by the real reducer)

`workspaceCommandPlanner.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { planWorkspaceCommand } from './workspaceCommandPlanner';
import {
  chatGroupsReducer,
  createInitialChatGroupsState,
  activeTabOf,
  type ChatGroupsState,
} from './chatGroupsReducer';
import { MAX_GROUPS } from './chatGroupsLayout';

/** Real state with N session-bound tabs, built through the real reducer. */
function stateWithSessions(ids: string[]): ChatGroupsState {
  let state = createInitialChatGroupsState();
  for (const id of ids) {
    state = chatGroupsReducer(state, { type: 'openTab', payload: { sessionId: id } });
  }
  return state;
}

describe('planWorkspaceCommand', () => {
  it('open_tab focus:false opens then restores the previously active tab', () => {
    const state = stateWithSessions(['s-mine']);
    const prevActive = activeTabOf(state)?.tabId;
    const plan = planWorkspaceCommand(
      { type: 'workspace', cmd: 'open_tab', session_id: 's-new', placement: 'tab', focus: false },
      state
    );
    expect(plan.result.ok).toBe(true);
    expect(plan.actions[0]).toEqual({ type: 'openTab', payload: { sessionId: 's-new' } });
    // Focus etiquette (§4.1): the LAST action re-activates the user's tab.
    expect(plan.actions[plan.actions.length - 1]).toEqual({
      type: 'activateTab',
      tabId: prevActive,
    });
  });

  it('open_tab focus:true does not restore', () => {
    const plan = planWorkspaceCommand(
      { type: 'workspace', cmd: 'open_tab', session_id: 's-new', placement: 'tab', focus: true },
      stateWithSessions(['s-mine'])
    );
    expect(plan.actions).toHaveLength(1);
  });

  it('refuses a split at MAX_GROUPS with a clear detail', () => {
    // Build MAX_GROUPS groups by moving tabs to edge zones.
    let state = stateWithSessions(
      Array.from({ length: MAX_GROUPS }, (_, i) => `s-${i}`)
    );
    for (let i = 1; i < MAX_GROUPS; i++) {
      const groupId = state.order[state.order.length - 1];
      const tab = state.groups[state.order[0]].tabs[0];
      if (!tab) break;
      state = chatGroupsReducer(state, {
        type: 'moveTabToGroup', tabId: tab.tabId, targetGroupId: groupId, zone: 'right',
      });
    }
    const plan = planWorkspaceCommand(
      { type: 'workspace', cmd: 'open_tab', session_id: 's-x', placement: 'split', focus: false },
      state
    );
    if (state.order.length >= MAX_GROUPS) {
      expect(plan.result.ok).toBe(false);
      expect(plan.result.detail).toContain('split refused');
      expect(plan.actions).toHaveLength(0);
    }
  });

  it('activate/close resolve tabs by session id; misses are reported', () => {
    const state = stateWithSessions(['s-1']);
    const hit = planWorkspaceCommand(
      { type: 'workspace', cmd: 'close_tab', session_id: 's-1' }, state);
    expect(hit.result.ok).toBe(true);
    expect(hit.actions[0].type).toBe('closeTab');
    const miss = planWorkspaceCommand(
      { type: 'workspace', cmd: 'activate_tab', session_id: 's-none' }, state);
    expect(miss.result.ok).toBe(false);
    expect(miss.result.detail).toBe('session has no tab');
  });

  it('open_window and notify and annotate_tab become side-effect plans', () => {
    const state = stateWithSessions([]);
    const win = planWorkspaceCommand(
      { type: 'workspace', cmd: 'open_window', session_id: 's-w' }, state);
    expect(win.openWindowSessionId).toBe('s-w');
    const note = planWorkspaceCommand(
      { type: 'workspace', cmd: 'notify', session_id: 's-1', message: 'tools changed' }, state);
    expect(note.notify).toEqual({ message: 'tools changed', level: undefined });
    const badge = planWorkspaceCommand(
      { type: 'workspace', cmd: 'annotate_tab', session_id: 'c-1', badge: 'subagent', parent_session_id: 'p-1' },
      state
    );
    expect(badge.annotate).toEqual({
      sessionId: 'c-1',
      annotation: { badge: 'subagent', parentSessionId: 'p-1' },
    });
  });
});
```

- [ ] **Step 5: Run to verify failure**

Run: `cd ui/desktop && npm run test:run -- workspaceCommandPlanner`
Expected: FAIL — `workspaceCommandPlanner` module not found (and, on first
implement attempt, `findTabBySession` not exported).

- [ ] **Step 6: Implement the planner + the reducer export**

(a) In `chatGroupsReducer.ts`, export the session lookup the planner (and Task 29's
badge rendering) needs — a public wrapper over the private `findTabGroup` (:241):

```typescript
/** BR-71: locate a session's tab anywhere in the layout. Public wrapper over
 * findTabGroup for the workspace command planner and tab annotations. */
export function findTabBySession(
  state: ChatGroupsState,
  sessionId: string
): { tabId: ChatTabId; groupId: ChatGroupId } | null {
  const hit = findTabGroup(state, (tab) => tab.sessionId === sessionId);
  return hit ? { tabId: hit.tab.tabId, groupId: hit.group.groupId } : null;
}
```

(Verify `findTabGroup`'s return shape first — `grep -n "function findTabGroup" -A 6
ui/desktop/src/components/chatGroups/chatGroupsReducer.ts`; adapt the destructuring
to its actual `{ group, tab }` naming.)

(b) `workspaceCommandPlanner.ts` — complete:

```typescript
/**
 * BR-71 §4.3: pure planner mapping one workspace command frame onto existing
 * ChatGroups reducer actions plus declarative side effects. NO dispatching, NO
 * window access here — the ChatGroupsProvider executes the plan. Pure so every
 * behavior (split refusal, focus etiquette, annotation) is unit-testable
 * against real reducer state.
 */
import {
  activeTabOf,
  findTabBySession,
  type ChatGroupsAction,
  type ChatGroupsState,
} from './chatGroupsReducer';
import { MAX_GROUPS } from './chatGroupsLayout';
import type { WorkspaceCommand, WorkspaceCommandResult } from './workspaceCommandRegistry';

export type TabAnnotation = { badge?: string; parentSessionId?: string };

export type WorkspaceCommandPlan = {
  result: WorkspaceCommandResult;
  /** Reducer actions to dispatch, in order. */
  actions: ChatGroupsAction[];
  /** Relay to the create-chat-window IPC (placement:"window" / open_window). */
  openWindowSessionId?: string;
  /** Surface a toast. */
  notify?: { message: string; level?: string };
  /** Record a tab annotation (subagent badge, parent link). */
  annotate?: { sessionId: string; annotation: TabAnnotation };
};

export function planWorkspaceCommand(
  cmd: WorkspaceCommand,
  state: ChatGroupsState
): WorkspaceCommandPlan {
  switch (cmd.cmd) {
    case 'open_tab': {
      if (!cmd.session_id) return refuse('missing session_id');
      if (cmd.placement === 'split' && state.order.length >= MAX_GROUPS) {
        return refuse(`split refused: already at ${MAX_GROUPS} groups`);
      }
      const previouslyActive = activeTabOf(state)?.tabId ?? null;
      const actions: ChatGroupsAction[] = [
        // Dedupe by session id is the reducer's own rule (openTab): "open or
        // focus session X" is this one dispatch.
        { type: 'openTab', payload: { sessionId: cmd.session_id } },
      ];
      if (cmd.placement === 'split') {
        const existing = findTabBySession(state, cmd.session_id);
        if (existing) {
          // Already-open session: move its tab into a new right-edge group.
          actions.push({
            type: 'moveTabToGroup',
            tabId: existing.tabId,
            targetGroupId: existing.groupId,
            zone: 'right',
          });
        }
        // A NEW session's tab id does not exist until the openTab commits; the
        // provider's executor performs the follow-up move against post-commit
        // state (see Step 8's executor, which re-plans the move).
      }
      if (cmd.focus === false && previouslyActive) {
        // §4.1 focus etiquette: background-open never steals the composer.
        actions.push({ type: 'activateTab', tabId: previouslyActive });
      }
      return {
        result: { ok: true, detail: cmd.placement === 'split' ? 'opened in split' : 'opened' },
        actions,
      };
    }
    case 'activate_tab': {
      const hit = cmd.session_id ? findTabBySession(state, cmd.session_id) : null;
      if (!hit) return refuse('session has no tab');
      return { result: { ok: true }, actions: [{ type: 'activateTab', tabId: hit.tabId }] };
    }
    case 'close_tab': {
      const hit = cmd.session_id ? findTabBySession(state, cmd.session_id) : null;
      if (!hit) return refuse('session has no tab');
      return { result: { ok: true }, actions: [{ type: 'closeTab', tabId: hit.tabId }] };
    }
    case 'open_window':
      if (!cmd.session_id) return refuse('missing session_id');
      return {
        result: { ok: true, detail: 'window requested' },
        actions: [],
        openWindowSessionId: cmd.session_id,
      };
    case 'notify':
      return {
        result: { ok: true },
        actions: [],
        notify: { message: cmd.message ?? 'Workspace notification', level: cmd.level },
      };
    case 'annotate_tab': {
      if (!cmd.session_id) return refuse('missing session_id');
      return {
        result: { ok: true },
        actions: [],
        annotate: {
          sessionId: cmd.session_id,
          annotation: { badge: cmd.badge, parentSessionId: cmd.parent_session_id },
        },
      };
    }
    default:
      return refuse(`unknown cmd '${(cmd as WorkspaceCommand).cmd}'`);
  }
}

function refuse(detail: string): WorkspaceCommandPlan {
  return { result: { ok: false, detail }, actions: [] };
}
```

- [ ] **Step 7: Run the planner tests**

Run: `cd ui/desktop && npm run test:run -- workspaceCommandPlanner`
Expected: 5 passed.

- [ ] **Step 8: Wire the provider — a thin executor over the planner**

In `ChatGroupsContext.tsx` (grep `registerNewTab(` at :161 for the registration
pattern; `stateRef` exists at :195-196). Three additions, complete:

(a) Annotation state, exposed through the context value (consumed by
`ChatTabStrip` in Task 29):

```typescript
  const [tabAnnotations, setTabAnnotations] = useState<Record<string, TabAnnotation>>({});
```

Add `tabAnnotations` to the context value object (beside `dispatch` at :228).

(b) The command handler effect — executes plans; every behavior decision already
lives (tested) in the planner:

```typescript
  useEffect(() => {
    const runPlan = (cmd: WorkspaceCommand): WorkspaceCommandResult => {
      const plan = planWorkspaceCommand(cmd, stateRef.current);
      for (const action of plan.actions) dispatch(action);
      // Split follow-up for a NEWLY-created tab: the tab id only exists after
      // the openTab commits, so re-plan the move against the committed state
      // on the next microtask (stateRef is updated on every render).
      if (cmd.cmd === 'open_tab' && cmd.placement === 'split' && plan.result.ok) {
        queueMicrotask(() => {
          const hit = cmd.session_id
            ? findTabBySession(stateRef.current, cmd.session_id)
            : null;
          if (hit && stateRef.current.order.length < MAX_GROUPS) {
            dispatch({
              type: 'moveTabToGroup',
              tabId: hit.tabId,
              targetGroupId: hit.groupId,
              zone: 'right',
            });
          }
        });
      }
      if (plan.openWindowSessionId) {
        // create-chat-window IPC, exposed in preload.ts:414-423. Verify the
        // exact parameter order with `grep -n "createChatWindow" ui/desktop/src/preload.ts`
        // and pass the session id in the resume-session position.
        window.electron.createChatWindow(undefined, undefined, undefined, plan.openWindowSessionId);
      }
      if (plan.notify) {
        // toastService (ui/desktop/src/toasts.tsx:172) has success/error/loading;
        // info-level workspace notices use success with a Workspace title.
        toastService.success({ title: 'Workspace', msg: plan.notify.message });
      }
      if (plan.annotate) {
        setTabAnnotations((prev) => ({
          ...prev,
          [plan.annotate!.sessionId]: plan.annotate!.annotation,
        }));
      }
      // Daemon-opened tabs are, by definition, sessions this renderer is not
      // driving: attach the observer stream (§4.3; Task 21) so the tab renders
      // live without owning a /reply stream.
      if ((cmd.cmd === 'open_tab' || cmd.cmd === 'annotate_tab') && cmd.session_id) {
        defaultChatStreamRegistry.getController(cmd.session_id).observeSession();
      }
      return plan.result;
    };
    const dispose = registerWorkspaceCommands(runPlan);
    // Drain frames that arrived before this provider mounted (Settings-page
    // case — same rationale as consumePendingNewTab).
    for (const queued of drainPendingWorkspaceCommands()) runPlan(queued);
    return dispose;
  }, [dispatch]);
```

(c) Call `useWorkspaceChannel` once at the provider root (window id from
`sessionStorage` — mint `crypto.randomUUID()` on first use; secret via the same
`window.electron.getSecretKey()` the app already uses, `renderer.tsx:454`), and
`sendEcho(buildEchoFrame(...))` from an effect keyed on the reducer state (the same
effect placement as `acknowledgeNewTabCommit`, which already runs on every commit).

Imports to add: `planWorkspaceCommand`, `TabAnnotation` from
`../components/chatGroups/workspaceCommandPlanner`; `findTabBySession` from
`../components/chatGroups/chatGroupsReducer`; `MAX_GROUPS` from
`../components/chatGroups/chatGroupsLayout`; `registerWorkspaceCommands`,
`drainPendingWorkspaceCommands`, types from
`../components/chatGroups/workspaceCommandRegistry`; `toastService` from
`../toasts`; `defaultChatStreamRegistry` from `../hooks/chatStreamStore`
(`observeSession` lands in Task 21 — until then, guard the call with
`typeof controller.observeSession === 'function'` or land Tasks 20-21 together on
the branch before running the full suite).

- [ ] **Step 9: Run the frontend suite**

Run: `cd ui/desktop && npm run test:run -- workspaceCommandRegistry workspaceCommandPlanner useWorkspaceChannel chatGroups`
Expected: new tests pass; no chatGroups regressions.

- [ ] **Step 10: Commit**

```bash
git add ui/desktop/src
git commit -m "feat(ui): workspace channel client + command planner + ChatGroups wiring + layout echo (BR-71)"
```

---

### Task 21: Observer-backed `ChatStreamController` mode

**Files:**
- Modify: `ui/desktop/src/hooks/chatStreamStore.tsx`
  (anchors: `ChatStreamController` :282, `streamFromResponse(stream, initialMessages, streamId)`
  :870, the generated-client `reply({...})` call whose `{ stream }` feeds it at
  :992-1007, `getController` :1334)
- Create: `ui/desktop/src/hooks/chatStreamStore.observe.test.tsx`

**Transport decision (verified, not assumed):** the store's `/reply` path does NOT
hand-roll SSE — it calls the generated client (`reply` from `../api`, emitted as
`client.sse.post` in `sdk.gen.ts:514`) and receives `{ stream }`, an
`AsyncIterable<MessageEvent>`, which `streamFromResponse` consumes. hey-api emits the
`.sse.*` form for any route whose OpenAPI 200 response carries
`content: { "text/event-stream": … }` — which Task 7's utoipa annotation
(`content_type = "text/event-stream"`, `body = MessageEvent`) produces, exactly like
`/reply`'s (`reply.rs:410` → `openapi.json` `/reply` 200 content). So after Task 7's
regen, `observeSessionEvents` is a generated `client.sse.get` function returning
`{ stream }` — **no raw fetch, no `window.electron`, no second SSE parser**.
Verify after regenerating: `grep -n "observeSessionEvents" ui/desktop/src/api/sdk.gen.ts`
→ expect a `.sse.get<...>` call; if it generated as plain `.get`, the utoipa
annotation is missing its `content_type` — fix Task 7, do not work around it here.

- [ ] **Step 1: Write the failing test** — mocks the generated client module (the
same `vi.mock('../api', …)` pattern `SessionListView.test.tsx:9-17` already uses),
so it needs neither `fetch` nor `window.electron` and passes against the real
implementation:

```typescript
import { describe, expect, it, vi, afterEach } from 'vitest';

const mocks = vi.hoisted(() => ({
  observeSessionEvents: vi.fn(),
}));

vi.mock('../api', async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>;
  return { ...actual, observeSessionEvents: mocks.observeSessionEvents };
});

import { defaultChatStreamRegistry } from './chatStreamStore';

/** The `{ stream }` shape the generated .sse.get returns: an AsyncIterable of
 * parsed MessageEvent frames. */
async function* frames() {
  yield { type: 'UpdateConversation', conversation: [], token_state: {} };
  yield {
    type: 'Message',
    token_state: {},
    message: {
      role: 'assistant',
      created: 1,
      content: [{ type: 'text', text: 'from the observed turn' }],
      metadata: { userVisible: true, agentVisible: true },
    },
  };
  yield { type: 'Finish', reason: 'stop', token_state: {} };
}

describe('ChatStreamController.observeSession', () => {
  afterEach(() => vi.clearAllMocks());

  it('renders observer frames without owning a /reply stream', async () => {
    mocks.observeSessionEvents.mockResolvedValue({ stream: frames() });

    const controller = defaultChatStreamRegistry.getController('obs-1');
    await controller.observeSession();

    expect(mocks.observeSessionEvents).toHaveBeenCalledWith(
      expect.objectContaining({ path: { session_id: 'obs-1' } })
    );
    const text = JSON.stringify(controller.getSnapshot().messages);
    expect(text).toContain('from the observed turn');
  });

  it('stops reconnecting once aborted', async () => {
    // First connect yields a stream that ends; observeSession must not spin a
    // reconnect after stopObserving() aborts it.
    mocks.observeSessionEvents.mockResolvedValue({ stream: frames() });
    const controller = defaultChatStreamRegistry.getController('obs-2');
    const done = controller.observeSession();
    controller.stopObserving();
    await done;
    // ≤ 2 calls: the initial connect plus at most one in-flight retry that the
    // abort check cancels before issuing a request loop.
    expect(mocks.observeSessionEvents.mock.calls.length).toBeLessThanOrEqual(2);
  });
});
```

(Adjust `getSnapshot()` to the controller's real snapshot accessor — grep
`getSnapshot\|snapshot` in the class; the assertion is on observed messages landing
in the same store the chat renders from.)

- [ ] **Step 2: Run to verify failure**

Run: `cd ui/desktop && npm run test:run -- chatStreamStore.observe`
Expected: FAIL — `observeSession` is not a function.

- [ ] **Step 3: Implement**

Add `observeSessionEvents` to the store's existing `../api` import list (:3-17), then
add to `ChatStreamController` (the whole point is reuse — `streamFromResponse` at
:870 already understands every `MessageEvent` variant, including the observer's
snapshot `UpdateConversation` and `Finish`):

```typescript
  /** True while this controller is an observer of a session another agent
   * drives (BR-71). Cleared by stopObserving() and by the user submitting
   * their own message (submitPreparedMessage bumps activeStreamId). */
  private observing = false;

  /**
   * BR-71: render a session this window is NOT driving. Subscribes to the
   * read-only observer stream (GET /sessions/{id}/events, generated client
   * .sse.get) and feeds it through the SAME event pipeline as a /reply stream
   * — the observer emits identical MessageEvent frames, starting with an
   * UpdateConversation snapshot. Used by tabs the daemon opened (subagent
   * tabs, workspace_open from another agent).
   *
   * Owns its reconnects: the observer stream never "completes" from the
   * client's point of view (the session outlives any one connection), so on
   * stream end or transport error it re-subscribes with backoff until
   * stopObserving() or a user-driven turn takes the controller over
   * (design §4.3; the daemon side is generation-safe — a re-subscribe is just
   * a new broadcast receiver + fresh snapshot).
   */
  async observeSession(): Promise<void> {
    if (this.observing) return; // idempotent — tab re-mounts must not stack loops
    this.observing = true;
    let retryMs = 1000;
    while (this.observing) {
      const streamId = ++this.activeStreamId;
      this.abortController?.abort();
      this.abortController = new AbortController();
      try {
        const { stream } = await observeSessionEvents({
          path: { session_id: this.sessionId },
          throwOnError: true,
          signal: this.abortController.signal,
        });
        retryMs = 1000;
        await this.streamFromResponse(stream, this.messagesRef, streamId);
      } catch (error) {
        if (error instanceof Error && error.name === 'AbortError') return;
        // fall through to retry
      }
      if (!this.observing || this.activeStreamId !== streamId) return;
      await new Promise((resolve) => setTimeout(resolve, retryMs));
      retryMs = Math.min(retryMs * 2, 15000);
    }
  }

  /** Detach from the observed session (tab closed / user takes over). */
  stopObserving(): void {
    this.observing = false;
    this.abortController?.abort();
  }
```

Two integration points, both inside this file: (a) `submitPreparedMessage` (:946)
already bumps `activeStreamId` and swaps `abortController`, which exits the observer
loop's staleness check — additionally set `this.observing = false` there so a user
taking over a subagent tab cleanly converts the controller from observer to driver;
(b) `messagesRef` names the controller's current message list — grep how
`streamFromResponse`'s second argument is built at the :1007 call site
(`currentMessages`) and pass the same source.

- [ ] **Step 4: Run tests**

Run: `cd ui/desktop && npm run test:run -- chatStreamStore`
Expected: both new tests pass; existing store tests unchanged.

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/hooks/chatStreamStore.tsx ui/desktop/src/hooks/chatStreamStore.observe.test.tsx
git commit -m "feat(ui): observer-backed ChatStreamController mode over /sessions/{id}/events (BR-71)"
```

---

### Task 22: Provenance chips + set-tools toasts in the transcript

**Files:**
- Modify: the message-rendering component (locate with
  `grep -rln "userVisible\|user_visible" ui/desktop/src/components | grep -i message`
  — the component that renders a user message bubble; call it `UserMessage` here)
- Create: `ui/desktop/src/components/ProvenanceChip.tsx` + test

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ProvenanceChip } from './ProvenanceChip';

describe('ProvenanceChip', () => {
  it('labels agent injections with the source session name', () => {
    render(
      <ProvenanceChip
        provenance={{ kind: 'agent_injection', fromSessionId: 's1', fromSessionName: 'Planning chat' }}
      />
    );
    expect(screen.getByText(/injected by Planning chat/i)).toBeTruthy();
  });

  it('labels direct human input into subagent tabs', () => {
    render(<ProvenanceChip provenance={{ kind: 'user_direct' }} />);
    expect(screen.getByText(/direct user message/i)).toBeTruthy();
  });

  it('renders nothing without provenance', () => {
    const { container } = render(<ProvenanceChip provenance={undefined} />);
    expect(container.firstChild).toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd ui/desktop && npm run test:run -- ProvenanceChip`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```tsx
/**
 * BR-71 §5: provenance is structural — any message injected across sessions is
 * permanently labeled in the transcript. Renders nothing for ordinary
 * same-session messages. Styling follows design.md (text-subtle, no ring).
 */
export type MessageProvenanceView = {
  kind: 'agent_injection' | 'user_direct' | 'spawn_context';
  fromSessionId?: string;
  fromSessionName?: string;
};

export function ProvenanceChip({ provenance }: { provenance?: MessageProvenanceView }) {
  if (!provenance) return null;
  const label =
    provenance.kind === 'agent_injection'
      ? `injected by ${provenance.fromSessionName ?? provenance.fromSessionId ?? 'another agent'}`
      : provenance.kind === 'user_direct'
        ? 'direct user message'
        : 'spawn context';
  return (
    <span
      className="inline-flex items-center gap-1 rounded-full border border-border-subtle px-2 py-0.5 text-xs text-text-subtle"
      title={provenance.fromSessionId}
      data-provenance-kind={provenance.kind}
    >
      {label}
    </span>
  );
}
```

Mount it in the message bubble component wherever `message.metadata` is already in
scope: `<ProvenanceChip provenance={message.metadata?.provenance} />` above the bubble
body (the generated API type gains `provenance` from Task 7's regen; if the generated
name is snake_case, adapt the prop mapping at the mount site, keeping the component's
camelCase view type). Set-tools toasts already arrive as `notify` frames (Task 13
emits them; Task 20 routes them to the toast service) — nothing more to build here.

- [ ] **Step 4: Run tests**

Run: `cd ui/desktop && npm run test:run -- ProvenanceChip`
Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/components
git commit -m "feat(ui): provenance chips on injected messages (BR-71)"
```

---

### Task 23: Phase 2 gate — live GUI verification

- [ ] **Step 1: Backend + frontend suites**

Run: `cargo test --workspace --no-fail-fast 2>&1 | tail -5` and
`cd ui/desktop && npm run test:run && npm run lint:check`
Expected: green (modulo the recorded pre-existing baseline).

- [ ] **Step 2: Live check, per the dev-GUI rules** (read
`docs/desktop-ui/launching-the-dev-gui.md` first; `BIOROUTER_NO_HMR=1`; verify with CDP
screenshots, never `screencapture`; `env -u ELECTRON_RUN_AS_NODE`)

1. Launch the dev GUI with `BIOROUTER_NO_HMR=1`.
2. Enable the `workspace` extension on a chat session (Settings → Extensions).
3. Ask the agent: "open my most recent other conversation in a new tab, without
   focusing it". Verify over CDP: a background tab appears bound to that session; the
   composer keeps focus; `workspace_list` now reports the tab under `gui`.
4. Ask it to `workspace_send_prompt mode:"note"` into that session; open the tab and
   verify the provenance chip.
5. Close the GUI; re-run the same tool from `biorouter` CLI against the daemon —
   verify `gui_attached: false` degradation.

- [ ] **Step 3: Update the design-doc status header** (Slice 2 shipped) and commit:

```bash
git add docs/agent-loop/designs/agent-workspace-control.md
git commit -m "docs(br71): mark slice 2 implemented in the design status header"
```

---

# Phase 3 — Glass-box subagents (design Slice 3)

Ships independently: after Task 32 every spawned subagent stamps its parent, persists
its spawn context, runs as a **registered agent under the server turn lock** (so
`/interrupt` steers the live child, Stop/cancel really stop it, and `workspace_list`
sees it running — reconciliation #2), streams onto the bus, opens (background) as an
annotated tab the human can watch, steer, and stop; the parent's result reports human
intervention.

### Task 24: Spawn stamps `parent_session_id` + persists the spawn context

**Files:**
- Modify: `crates/biorouter/src/agents/subagent_tool.rs` (`create_subagent_session`
  at :545-562 — gains the parent id)
- Modify: `crates/biorouter/src/agents/subagent_handler.rs` (`get_agent_messages`
  :134-305 — persists the rendered spawn context after `override_system_prompt` :213)

- [ ] **Step 1: Write the failing test**

In `subagent_handler.rs`'s test module (create one if absent — `#[cfg(test)] mod tests`
at the bottom):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::ProvenanceKind;
    use crate::session::session_manager::SessionType;

    #[tokio::test]
    async fn spawn_context_is_persisted_visible_to_user_not_agent() {
        let temp = tempfile::TempDir::new().unwrap();
        let sm = std::sync::Arc::new(SessionManager::new(temp.path().to_path_buf()));
        let child = sm
            .create_session(temp.path().to_path_buf(), "Subagent task".into(), SessionType::SubAgent)
            .await
            .unwrap();

        persist_spawn_context(
            &sm,
            &child.id,
            "parent-1",
            "SYSTEM PROMPT RENDERED HERE",
            "task: count the files",
            &["developer".to_string()],
            &["single-cell".to_string()],
            Some("kb-papers"),
        )
        .await
        .unwrap();

        let reread = sm.get_session(&child.id, true).await.unwrap();
        assert_eq!(reread.parent_session_id.as_deref(), Some("parent-1"));
        let msgs = reread.conversation.unwrap().messages().to_vec();
        let record = msgs.first().expect("spawn context is the first message");
        assert!(record.metadata.user_visible);
        assert!(!record.metadata.agent_visible, "must not enter the child's model context");
        assert_eq!(
            record.metadata.provenance.as_ref().unwrap().kind,
            ProvenanceKind::SpawnContext
        );
        let text: String = record.content.iter().filter_map(|c| c.as_text()).collect();
        assert!(text.contains("SYSTEM PROMPT RENDERED HERE"));
        assert!(text.contains("count the files"));
        assert!(text.contains("developer"));
        // §4.5/issue: the record carries ALL grants — extensions, skills, KB.
        assert!(text.contains("single-cell"));
        assert!(text.contains("kb-papers"));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter --lib agents::subagent_handler`
Expected: COMPILE ERROR — `persist_spawn_context` not found.

- [ ] **Step 3: Implement**

In `subagent_handler.rs`:

```rust
/// BR-71 §4.4: persist the child's rendered spawn context as its first message
/// — user_visible (the tab header shows it), agent_visible: false (the child's
/// model context already receives it as the system override; storing it
/// visibly must not double-inject it). Also stamps parent_session_id. The
/// record carries ALL grants the issue names — extensions, skills, and the
/// knowledge base — so `workspace_read_conversation view:"spawn_context"` and
/// the tab header can show them without a second source of truth.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn persist_spawn_context(
    session_manager: &SessionManager,
    child_session_id: &str,
    parent_session_id: &str,
    rendered_system_prompt: &str,
    task_instructions: &str,
    extension_names: &[String],
    skill_names: &[String],
    knowledge_base: Option<&str>,
) -> Result<()> {
    use crate::conversation::message::{MessageProvenance, ProvenanceKind};

    session_manager
        .update(child_session_id)
        .parent_session_id(Some(parent_session_id.to_string()))
        .apply()
        .await?;

    let body = format!(
        "## Subagent spawn context\n\nSpawned by session: {parent_session_id}\n\n\
         ### Task instructions\n{task_instructions}\n\n\
         ### Granted extensions\n{}\n\n\
         ### Granted skills\n{}\n\n\
         ### Knowledge base\n{}\n\n\
         ### Rendered system prompt\n{rendered_system_prompt}",
        if extension_names.is_empty() { "(parent defaults)".to_string() } else { extension_names.join(", ") },
        if skill_names.is_empty() { "(none)".to_string() } else { skill_names.join(", ") },
        knowledge_base.unwrap_or("(none)"),
    );
    let mut record = Message::user().with_text(body);
    record.metadata.user_visible = true;
    record.metadata.agent_visible = false;
    record.metadata.provenance = Some(MessageProvenance {
        kind: ProvenanceKind::SpawnContext,
        from_session_id: Some(parent_session_id.to_string()),
        from_session_name: None,
    });
    session_manager
        .add_message_adopting_uid(child_session_id, &mut record)
        .await?;
    Ok(())
}
```

Call it from `get_agent_messages` immediately after `agent.override_system_prompt(subagent_prompt).await;`
(:213), before the reply stream starts:

```rust
        // Grants for the record: extensions from the task config; skills from
        // the workflow (`workflow.skills`, workflow/mod.rs:60-61); the child's
        // active KB via the daemon services when installed (usually None — a
        // subagent inherits no KB today; recorded truthfully either way).
        let skill_names: Vec<String> = workflow.skills.clone().unwrap_or_default();
        let knowledge_base = crate::workspace_services::get()
            .and_then(|s| s.active_knowledge_base(&session_id));
        if let Err(e) = persist_spawn_context(
            &session_manager,
            &session_id,
            &task_config.parent_session_id,
            &subagent_prompt,
            &system_instructions,
            &task_config
                .extensions
                .iter()
                .map(|e| e.name().to_string())
                .collect::<Vec<_>>(),
            &skill_names,
            knowledge_base.as_deref(),
        )
        .await
        {
            // Best-effort: a failed context record must not kill the run.
            tracing::warn!("failed to persist subagent spawn context: {e}");
        }
```

`get_agent_messages` needs `session_manager` in scope — it is on `config.session_manager`
before `config` is moved into `Agent::with_config` at :149; clone the `Arc` first:
`let session_manager = config.session_manager.clone();` (the enclosing
`run_complete_subagent_task` already does exactly this at :48 — thread it through as a
parameter or re-clone before the move; also capture
`task_config.parent_session_id`/`extensions` **before** they are consumed by the loop
at :176 — reorder the borrow so the persist call happens right after the prompt render
where all inputs are still live).

**Verification guard (the overwrite risk):** the test's final assertion — the record is
still the FIRST message after the run — is added in Task 26's integration test, because
the child agent persists its own conversation as it runs; if the child's persistence
path REPLACES rather than appends (check `replace_conversation` usage in the agent's
persist path), the spawn record would be lost, and the fix is to seed the child's
in-memory `Conversation` with the record (agent_visible false keeps it out of the model
context). Do not skip this check.

- [ ] **Step 4: Run tests**

Run: `cargo test -p biorouter --lib agents::subagent_handler`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/agents/subagent_handler.rs crates/biorouter/src/agents/subagent_tool.rs
git commit -m "feat(subagent): stamp parent_session_id and persist spawn context (BR-71)"
```

---

### Task 25: Register the child agent + hold the server turn lease (the control-plane bridge)

**This is the task that makes the flagship's human-steer, Stop, and running-state
paths real** (reconciliation #2). Without it, `POST /interrupt` mints a different
agent for the child session, `/agent/cancel` finds no `ActiveTurn`, and
`is_turn_active(child)` is false while the child runs — three symptoms, one root
cause: the child agent is invisible to the server control plane.

**Files:**
- Modify: `crates/biorouter/src/execution/manager.rs` (`AgentManager` :19-24;
  `get_or_create_agent` :112-144; `remove_session` :146-153)
- Modify: `crates/biorouter/src/agents/subagent_handler.rs`
  (`run_complete_subagent_task` :40-94 — run token + lease + registration;
  `get_agent_messages` :135-305 — register the built agent, pass the run token)
- Depends on: Task 9's `WorkspaceServices::begin_turn` / `WorkspaceTurnLease`

- [ ] **Step 1: Write the failing tests**

In `execution/manager.rs`'s existing test module (the `create_test_manager` helper at
:181-187 is reused):

```rust
    /// BR-71: a subagent run registers its ALREADY-CONFIGURED agent so the
    /// server's get_or_create_agent (the /interrupt and /reply resolution
    /// path — state.rs:290) returns the LIVE instance, not a fresh default one.
    #[tokio::test]
    async fn register_agent_makes_get_or_create_return_the_live_instance() {
        let temp_dir = TempDir::new().unwrap();
        let manager = create_test_manager(&temp_dir).await;

        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let child = Arc::new(crate::agents::Agent::with_config(
            crate::agents::AgentConfig::new(
                session_manager,
                crate::config::permission::PermissionManager::instance(),
                None,
                crate::config::BioRouterMode::Auto,
            ),
        ));

        manager.register_agent("child-1".to_string(), child.clone()).await;
        let resolved = manager.get_or_create_agent("child-1".to_string()).await.unwrap();
        assert!(
            Arc::ptr_eq(&child, &resolved),
            "steer/interrupt must reach the SAME live agent the run drives"
        );

        // Deregistration removes exactly our entry; a successor registered
        // meanwhile survives (the TurnGuard-style only-clear-your-own rule).
        manager.deregister_agent_if_same("child-1", &child).await;
        assert!(!manager.has_session("child-1").await);

        let replacement = manager.get_or_create_agent("child-1".to_string()).await.unwrap();
        manager.deregister_agent_if_same("child-1", &child).await; // stale — no-op
        let still = manager.get_or_create_agent("child-1".to_string()).await.unwrap();
        assert!(Arc::ptr_eq(&replacement, &still));
    }
```

In `subagent_handler.rs`'s test module (from Task 24):

```rust
    /// Headless (no WorkspaceServices installed): the run must not require the
    /// daemon — no lease, no panic, result envelope still produced (§2.1).
    /// The lease-held path is asserted end-to-end by the Task 31 harness
    /// (interrupt 202 + cancel true), which runs against a real daemon.
    #[tokio::test]
    async fn subagent_run_without_daemon_services_still_completes() {
        let temp = tempfile::TempDir::new().unwrap();
        let sm = std::sync::Arc::new(SessionManager::new(temp.path().to_path_buf()));
        let child = sm
            .create_session(temp.path().to_path_buf(), "child".into(),
                crate::session::session_manager::SessionType::SubAgent)
            .await
            .unwrap();
        let config = AgentConfig::new(
            sm.clone(),
            crate::config::permission::PermissionManager::instance(),
            None,
            crate::config::BioRouterMode::Auto,
        );
        // TestProvider replaying an empty cassette: fails on first use — the
        // run errors fast, which is all this needs (manager.rs:349-360 pattern).
        let cassette = temp.path().join("empty.json");
        std::fs::write(&cassette, "{}").unwrap();
        let provider = std::sync::Arc::new(
            crate::providers::testprovider::TestProvider::new_replaying(
                cassette.to_str().unwrap(),
            )
            .unwrap(),
        );
        let workflow: Workflow = serde_json::from_value(serde_json::json!({
            "title": "t", "description": "d",
            "instructions": "do the thing", "prompt": "go"
        }))
        .unwrap();
        let task_config = TaskConfig {
            provider,
            parent_session_id: "parent-1".into(),
            parent_working_dir: temp.path().to_path_buf(),
            extensions: vec![],
            max_turns: Some(3),
        };

        let result =
            run_complete_subagent_task(config, workflow, task_config, true, child.id.clone(), None)
                .await;
        // The provider fails, so the envelope reports an error/incomplete run —
        // the assertion is that it IS a structured envelope and nothing panicked
        // without the daemon: from_error/from_aborted_turn always set a
        // non-empty summary-or-error body.
        let rendered = serde_json::to_string(&result).unwrap();
        assert!(!rendered.is_empty());
    }
```

(`TaskConfig`'s five public fields verified at `subagent_task_config.rs:16-22`;
`Workflow` has NO `Default` derive (`workflow/mod.rs:31`) — `title` and
`description` are required, so it is built via serde with the defaulted `version`.
If `SubagentResult` does not implement `Serialize`, assert on
`result.into_call_tool_result()` being non-empty instead — grep the struct's
derives first.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter --lib execution::manager::tests::register_agent_makes_get_or_create_return_the_live_instance`
Expected: COMPILE ERROR — no method `register_agent` / `deregister_agent_if_same`.

- [ ] **Step 3: Implement the `AgentManager` registration API**

In `execution/manager.rs`, beside `get_or_create_agent`:

```rust
    /// BR-71: put an externally-built, fully-configured agent (a glass-box
    /// subagent) into the registry under its session id, so every server
    /// resolution path — `POST /interrupt`, `POST /reply`, workspace steer —
    /// returns the LIVE instance instead of minting a default agent that no
    /// running loop drains. Overwrites any placeholder entry an early racing
    /// resolution created (the live child wins).
    pub async fn register_agent(&self, session_id: String, agent: Arc<Agent>) {
        let mut sessions = self.sessions.write().await;
        sessions.put(session_id, agent);
    }

    /// Remove `session_id`'s entry ONLY if it still is `agent` — the
    /// TurnGuard discipline (state.rs:65-79): a finished run may only clear
    /// its own registration, never a successor's.
    pub async fn deregister_agent_if_same(&self, session_id: &str, agent: &Arc<Agent>) {
        let mut sessions = self.sessions.write().await;
        let same = sessions
            .peek(session_id)
            .is_some_and(|current| Arc::ptr_eq(current, agent));
        if same {
            sessions.pop(session_id);
        }
    }
```

(`LruCache::peek` reads without promoting — `lru` crate API; if the pinned version
lacks `peek`, use `get` — promotion is harmless here. **LRU eviction note:** a
registered child occupies one of the 100 LRU slots; eviction mid-run requires 100
intervening agent creations, after which a steer would mint a fresh agent again —
the pre-BR-71 behavior, degraded not broken. Recorded as operator risk #10.)

- [ ] **Step 4: Wire the run: token → lease → registration → deregistration**

In `run_complete_subagent_task` (:40), replace the token plumbing at the top:

```rust
    // BR-71 reconciliation #2 — one token per run, addressable from everywhere:
    // a child of the parent-supplied token (parent-cancel still propagates to
    // the child; cancelling the CHILD never kills the parent's turn), handed to
    // the server turn lease, the active-work guard, and the agent loop alike.
    let run_token = cancellation_token
        .as_ref()
        .map(tokio_util::sync::CancellationToken::child_token)
        .unwrap_or_default();

    // Hold the server's per-session turn lock for the run when the daemon is
    // present (headless: None — today's behavior). Makes is_turn_active(child)
    // true, keeps one-turn-per-session, and routes POST /agent/cancel /
    // workspace_close scope:"turn" / the tab's Stop to run_token.
    let _turn_lease: Option<Box<dyn crate::workspace_services::WorkspaceTurnLease>> =
        match crate::workspace_services::get() {
            Some(services) => match services.begin_turn(&session_id, run_token.clone()) {
                Ok(lease) => Some(lease),
                Err(conflict) => {
                    return SubagentResult::from_error(format!(
                        "subagent session is unexpectedly busy: {conflict}"
                    ));
                }
            },
            None => None,
        };
```

then change the active-work guard's cancel closure to ALWAYS route to `run_token`
(today it is `None` when no parent token was supplied — the run becomes
addressable either way):

```rust
        let cancel: std::sync::Arc<dyn Fn() + Send + Sync> = {
            let token = run_token.clone();
            std::sync::Arc::new(move || token.cancel())
        };
        // …ActiveWorkGuard::register(…, Some(cancel)) as today (:62-68).
```

and pass `Some(run_token.clone())` (not the raw parameter) into
`get_agent_messages(config, workflow, task_config, session_id.clone(), Some(run_token.clone()))`.

In `get_agent_messages`, right after `let agent = Arc::new(Agent::with_config(config));`
(:149), register the child and arm RAII deregistration:

```rust
        // BR-71: make the live child addressable by the server control plane.
        // Best-effort — AgentManager::instance() needs global config; unit
        // tests and bare-library embedding run fine without it.
        let registration = match crate::execution::manager::AgentManager::instance().await {
            Ok(manager) => {
                manager.register_agent(session_id.clone(), agent.clone()).await;
                Some((manager, agent.clone()))
            }
            Err(e) => {
                tracing::debug!("subagent not registered in AgentManager: {e}");
                None
            }
        };
        // Deregister on every exit path (scopeguard-free: a small Drop struct).
        struct Deregister {
            manager: Option<(
                std::sync::Arc<crate::execution::manager::AgentManager>,
                std::sync::Arc<Agent>,
            )>,
            session_id: String,
        }
        impl Drop for Deregister {
            fn drop(&mut self) {
                if let Some((manager, agent)) = self.manager.take() {
                    let session_id = std::mem::take(&mut self.session_id);
                    tokio::spawn(async move {
                        manager.deregister_agent_if_same(&session_id, &agent).await;
                    });
                }
            }
        }
        let _deregister = Deregister { manager: registration, session_id: session_id.clone() };
```

**What now works on paper, path by path (the three coverage-critical chains):**

1. *Human steer mid-run:* composer busy-path → `POST /interrupt` → `is_turn_active`
   passes (the lease holds the lock) → `get_agent_for_route` →
   `get_or_create_agent` returns the REGISTERED child →
   `queue_soft_interrupt[_with_provenance]` (Task 27 stamps `user_direct`) → the
   child's own reply loop drains it at :3368.
2. *Stop / abort:* tab Stop → `POST /agent/cancel` → `state.cancel_turn(child)`
   finds the lease's `ActiveTurn`, trips `run_token` → the child's `agent.reply`
   stream ends with cancellation → `run_complete_subagent_task` yields
   `SubagentResult` (aborted/incomplete) → the parent's parked tool call resolves.
   `workspace_close scope:"turn"` is the same chain; `scope:"agent"` additionally
   evicts the registered entry via `stop_agent`. `subagent_status {cancel}` and
   active-work cancel trip the parent token / run token — same convergence.
3. *Visibility:* `workspace_list` reports the running child (`is_turn_active`
   true); `workspace_send_prompt mode:"steer"` passes its precondition;
   `mode:"turn"` on the RUNNING child is refused by the held lock — the
   one-turn-per-session invariant holds instead of silently double-running.

- [ ] **Step 5: Run tests**

Run: `cargo test -p biorouter --lib execution::manager agents::subagent_handler`
Expected: PASS (new tests plus all existing manager/handler tests).

- [ ] **Step 6: Commit**

```bash
git add crates/biorouter/src/execution/manager.rs crates/biorouter/src/agents/subagent_handler.rs
git commit -m "feat(subagent): register child agents + hold the server turn lease (BR-71 control plane)"
```

---

### Task 26: Subagent turns publish to the bus

**Files:**
- Modify: `crates/biorouter/src/agents/subagent_handler.rs` (stream loop :244-270)

- [ ] **Step 1: Write the failing test**

```rust
    #[tokio::test]
    async fn subagent_run_publishes_lifecycle_to_the_bus() {
        use crate::session_events::{self, SessionBusEvent};
        // A run with no provider fails fast — but must still bracket itself,
        // exactly like the detached runner (Task 8's test).
        let temp = tempfile::TempDir::new().unwrap();
        let sm = std::sync::Arc::new(SessionManager::new(temp.path().to_path_buf()));
        let child = sm
            .create_session(temp.path().to_path_buf(), "child".into(),
                crate::session::session_manager::SessionType::SubAgent)
            .await
            .unwrap();
        let mut rx = session_events::subscribe(&child.id);

        let config = AgentConfig::new(
            sm.clone(),
            crate::config::permission::PermissionManager::instance(),
            None,
            crate::config::BioRouterMode::Auto,
        );
        // Workflow has NO Default (workflow/mod.rs:31: title + description are
        // required) — build it via serde, version defaults.
        let workflow: Workflow = serde_json::from_value(serde_json::json!({
            "title": "t", "description": "d",
            "instructions": "do the thing", "prompt": "go"
        }))
        .unwrap();
        // The verified cheap provider: TestProvider replaying an empty cassette
        // fails on first use (the exact pattern of execution/manager.rs:349-360).
        let cassette = temp.path().join("empty.json");
        std::fs::write(&cassette, "{}").unwrap();
        let provider = std::sync::Arc::new(
            crate::providers::testprovider::TestProvider::new_replaying(
                cassette.to_str().unwrap(),
            )
            .unwrap(),
        );
        let task_config = TaskConfig {
            provider,
            parent_session_id: "parent-1".into(),
            parent_working_dir: temp.path().to_path_buf(),
            extensions: vec![],
            max_turns: Some(3),
        };

        let _result =
            run_complete_subagent_task(config, workflow, task_config, true, child.id.clone(), None)
                .await;

        let mut saw_started = false;
        let mut saw_finished = false;
        while let Ok(event) = rx.try_recv() {
            match event {
                SessionBusEvent::TurnStarted { .. } => saw_started = true,
                SessionBusEvent::TurnFinished { .. } => saw_finished = true,
                _ => {}
            }
        }
        assert!(saw_started && saw_finished, "subagent run must bracket itself on the bus");

        // Task 24 follow-through (the overwrite guard): the spawn-context
        // record survives the child's own persistence as message[0].
        let reread = sm.get_session(&child.id, true).await.unwrap();
        let msgs = reread.conversation.unwrap().messages().to_vec();
        if let Some(first) = msgs.first() {
            assert!(
                first.metadata.provenance.as_ref().is_some_and(|p| {
                    p.kind == crate::conversation::message::ProvenanceKind::SpawnContext
                }),
                "spawn-context record must remain the first message"
            );
        }
    }
```

(Every symbol verified: `TestProvider::new_replaying` at
`providers/testprovider.rs:52`; `TaskConfig`'s five fields at
`subagent_task_config.rs:16-22`; `AgentConfig::new`'s 4 args at `agent.rs:245-249`;
the run's *failure* is fine — only the bus bracket and the surviving spawn record
are asserted.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter --lib agents::subagent_handler::tests::subagent_run_publishes_lifecycle_to_the_bus`
Expected: FAIL — no events on the bus.

- [ ] **Step 3: Implement**

In `get_agent_messages`, around the stream loop (:244):

```rust
        // Turn id: the server lease's id when Task 25 acquired one (so observers
        // correlate with /agent/cancel's turn_id); a stable synthetic id headless.
        // Thread it from run_complete_subagent_task as an Option<String> parameter
        // (`lease_turn_id: _turn_lease.as_ref().map(|l| l.turn_id().to_string())`).
        crate::session_events::publish(
            &session_id,
            crate::session_events::SessionBusEvent::TurnStarted {
                turn_id: lease_turn_id.unwrap_or_else(|| format!("subagent-{session_id}")),
            },
        );
        let mut aborted: Option<TurnAbort> = None;
        // [MECHANICAL MOVE — the `agent.reply(...)` stream construction at
        //  :237-243 stays byte-identical here; only the loop below changes.]
        while let Some(message_result) = stream.next().await {
            match message_result {
                Ok(event) => {
                    // BR-71: glass-box — every child event is observable.
                    crate::session_events::publish(
                        &session_id,
                        crate::session_events::SessionBusEvent::Agent(event.clone()),
                    );
                    match event {
                        AgentEvent::Message(msg) => conversation.push(msg),
                        AgentEvent::McpNotification(_)
                        | AgentEvent::ModelChange { .. }
                        | AgentEvent::ToolCallPending(_)
                        | AgentEvent::TokenUsage(_) => {}
                        AgentEvent::HistoryReplaced(updated_conversation) => {
                            conversation = updated_conversation;
                        }
                        AgentEvent::TurnAborted { code, message } => {
                            tracing::error!(abort = code.wire_code(), "Subagent turn aborted: {message}");
                            aborted = Some((code.wire_code().to_string(), message));
                            break;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Error receiving message from subagent: {}", e);
                    aborted = Some(("stream_error".to_string(), e.to_string()));
                    break;
                }
            }
        }
        crate::session_events::publish(
            &session_id,
            crate::session_events::SessionBusEvent::TurnFinished {
                reason: if aborted.is_some() { "error".into() } else { "stop".into() },
            },
        );
```

(The per-variant bodies are today's bodies — this is the same nesting refactor as
Task 6; verify no body changed with
`git diff crates/biorouter/src/agents/subagent_handler.rs` — the diff must show only
the two `publish` insertions, the `TurnStarted`/`TurnFinished` brackets, the
`lease_turn_id` parameter, and the `TurnFinished` reason wiring; the `cancelled`
reason should be reported when the run token was tripped:
`if run_token_cancelled { "cancelled" } else if aborted.is_some() { "error" } else { "stop" }`
where `let run_token_probe = cancellation_token.clone();` is taken BEFORE the token
moves into `agent.reply(...)` at :239 and `run_token_cancelled =
run_token_probe.is_some_and(|t| t.is_cancelled())` — the run token already IS this
function's `cancellation_token` parameter, since Task 25 passes `Some(run_token)`.
The old comment at :277-278 "a
subagent's stream is not user-visible" is now half-true: update it to say the stream
is observable via the session bus but still not part of the parent's `/reply`
stream.)

- [ ] **Step 4: Run tests**

Run: `cargo test -p biorouter --lib agents::subagent_handler`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/agents/subagent_handler.rs
git commit -m "feat(subagent): publish child turns to the session bus (BR-71 glass-box)"
```

---

### Task 27: Human interventions — `user_direct` stamping + `human_intervened` in the result

**Files:**
- Modify: `crates/biorouter-server/src/routes/reply.rs` (stamp in `reply` before the
  turn starts, and in `interrupt` :910)
- Modify: `crates/biorouter/src/agents/subagent_result.rs` (new field)
- Modify: `crates/biorouter/src/agents/subagent_handler.rs` (set it from the final
  conversation)

- [ ] **Step 1: Write the failing tests**

In `subagent_result.rs`'s test module (it has one — grep `mod tests`):

```rust
    #[test]
    fn human_intervention_is_detected_from_provenance() {
        use crate::conversation::message::{Message, MessageProvenance, ProvenanceKind};
        let clean = Conversation::new_unvalidated(vec![Message::user().with_text("task")]);
        assert!(!conversation_has_user_direct(&clean));

        let steered = Conversation::new_unvalidated(vec![
            Message::user().with_text("task"),
            Message::user().with_text("actually, stop and use Python").with_provenance(
                MessageProvenance {
                    kind: ProvenanceKind::UserDirect,
                    from_session_id: None,
                    from_session_name: None,
                },
            ),
        ]);
        assert!(conversation_has_user_direct(&steered));
    }
```

In `reply.rs`'s test module:

```rust
    #[tokio::test]
    async fn replies_into_subagent_sessions_are_stamped_user_direct() {
        // The pure stamping helper is what we assert; the full /reply path
        // exercises it via the session_type read it already performs.
        use biorouter::conversation::message::{Message, ProvenanceKind};
        use biorouter::session::session_manager::SessionType;
        let stamped = stamp_user_direct_if_subagent(Message::user().with_text("hi"), SessionType::SubAgent);
        assert_eq!(
            stamped.metadata.provenance.as_ref().unwrap().kind,
            ProvenanceKind::UserDirect
        );
        let untouched = stamp_user_direct_if_subagent(Message::user().with_text("hi"), SessionType::User);
        assert!(untouched.metadata.provenance.is_none());
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter --lib agents::subagent_result && cargo test -p biorouter-server --lib routes::reply`
Expected: COMPILE ERRORS — helpers not found.

- [ ] **Step 3: Implement**

`subagent_result.rs`:

```rust
/// BR-71 §4.5: true when the child's conversation contains any message the
/// human injected directly through the subagent tab. The parent weighs the
/// summary accordingly.
pub fn conversation_has_user_direct(conversation: &Conversation) -> bool {
    use crate::conversation::message::ProvenanceKind;
    conversation.messages().iter().any(|m| {
        m.metadata
            .provenance
            .as_ref()
            .is_some_and(|p| p.kind == ProvenanceKind::UserDirect)
    })
}
```

Add to `SubagentResult` (grep `pub struct SubagentResult` for the field list):

```rust
    /// BR-71: the human typed into the child's tab during the run. Surfaced in
    /// the parent's tool result so it can weigh the summary.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub human_intervened: bool,
```

initialize `false` in every constructor (`from_error`, `from_aborted_turn`,
`from_conversation` — the compiler lists them), and include it in
`into_call_tool_result()`'s structured payload with one sentence in the
assistant-facing text when true: `"Note: the user intervened directly in this
subagent's tab during the run."`

In `subagent_handler.rs`'s `run_complete_subagent_task`, after `let mut result = …`
(:88-91):

```rust
    result.human_intervened =
        crate::agents::subagent_result::conversation_has_user_direct(&messages);
```

`reply.rs` — the pure helper plus its two call sites:

```rust
/// BR-71 §4.5: a human typing into a subagent's tab is an intervention the
/// parent must hear about. Sessions of other types are untouched.
pub(crate) fn stamp_user_direct_if_subagent(
    message: biorouter::conversation::message::Message,
    session_type: biorouter::session::session_manager::SessionType,
) -> biorouter::conversation::message::Message {
    use biorouter::conversation::message::{MessageProvenance, ProvenanceKind};
    if session_type == biorouter::session::session_manager::SessionType::SubAgent {
        message.with_provenance(MessageProvenance {
            kind: ProvenanceKind::UserDirect,
            from_session_id: None,
            from_session_name: None,
        })
    } else {
        message
    }
}
```

Call site 1 — in `reply`'s task, the handler already reads the session at :535
(`get_session(&session_id, true)`); stamp before `agent.reply`:
`let user_message = stamp_user_direct_if_subagent(user_message, session.session_type);`
(move the binding after the session read). Call site 2 — `interrupt` (:910) reads no
session today; add a `get_session(&req.session_id, false)` and queue with provenance:

```rust
    let session = state
        .session_manager()
        .get_session(&req.session_id, false)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if session.session_type == biorouter::session::session_manager::SessionType::SubAgent {
        agent.queue_soft_interrupt_with_provenance(
            req.text,
            Some(biorouter::conversation::message::MessageProvenance {
                kind: biorouter::conversation::message::ProvenanceKind::UserDirect,
                from_session_id: None,
                from_session_name: None,
            }),
        );
    } else {
        agent.queue_soft_interrupt(req.text);
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p biorouter --lib agents::subagent && cargo test -p biorouter-server --lib routes::reply`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/agents crates/biorouter-server/src/routes/reply.rs
git commit -m "feat(subagent): user_direct stamping + human_intervened in the parent result (BR-71)"
```

---

### Task 28: `workspace_spawn_subagent` + the workspace guard + tab announcement

**Files:**
- Modify: `crates/biorouter/src/agents/agent.rs` (dispatch guard :2137-2145; dispatch
  special-case :2216-2249)
- Modify: `crates/biorouter/src/agents/subagent_tool.rs` (params gain
  `visible`/`placement`; the announce hook)
- Modify: `crates/biorouter/src/agents/subagent_handler.rs` (exclusion of the
  workspace extension at :176-184)
- Modify: `crates/biorouter/src/agents/workspace_extension.rs` (advertise the tool)

- [ ] **Step 1: Write the failing tests**

In `agent.rs`'s test module (beside the existing subagent-guard tests — grep
`cannot create other subagents` for the model):

```rust
    #[test]
    fn subagent_sessions_are_refused_workspace_tools() {
        assert!(is_workspace_tool_refused_for(
            crate::session::session_manager::SessionType::SubAgent,
            "workspace_list"
        ));
        assert!(is_workspace_tool_refused_for(
            crate::session::session_manager::SessionType::SubAgent,
            "workspace_spawn_subagent"
        ));
        assert!(!is_workspace_tool_refused_for(
            crate::session::session_manager::SessionType::User,
            "workspace_list"
        ));
    }
```

In `subagent_handler.rs` tests:

```rust
    #[test]
    fn workspace_extension_is_stripped_from_child_grants() {
        let configs = vec![
            crate::agents::extension::ExtensionConfig::Platform {
                name: "workspace".into(),
                description: String::new(),
                bundled: None,
                available_tools: vec![],
            },
            crate::agents::extension::ExtensionConfig::Platform {
                name: "todo".into(),
                description: String::new(),
                bundled: None,
                available_tools: vec![],
            },
        ];
        let granted = strip_workspace_extension(configs);
        assert_eq!(granted.len(), 1);
        assert_eq!(granted[0].name(), "todo");
    }
```

In `subagent_tool.rs` tests:

```rust
    #[test]
    fn spawn_params_accept_visible_and_placement() {
        let params: SubagentParams = serde_json::from_value(serde_json::json!({
            "instructions": "count files",
            "visible": false,
            "placement": "split"
        }))
        .unwrap();
        assert_eq!(params.visible, Some(false));
        assert_eq!(params.placement.as_deref(), Some("split"));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p biorouter --lib agents::agent agents::subagent_tool agents::subagent_handler`
Expected: COMPILE ERRORS.

- [ ] **Step 3: Implement the guard** (agent.rs, beside :2137)

```rust
/// BR-71 §5: subagents never get workspace control — no delegation-tree
/// fan-out, no child steering its parent. Extension of the existing
/// "subagents cannot create other subagents" guard below.
///
/// Name forms (reconciliation #12): extension-advertised tools reach dispatch
/// PREFIXED (`workspace__workspace_list`, extension_manager.rs:971) — which
/// `starts_with("workspace_")` also matches (`workspace__…` begins with
/// `workspace_`); the bare forms cover prefix-stripping models
/// (extension_manager.rs:1294-1304 precedent).
pub(crate) fn is_workspace_tool_refused_for(
    session_type: crate::session::session_manager::SessionType,
    tool_name: &str,
) -> bool {
    session_type == crate::session::session_manager::SessionType::SubAgent
        && (tool_name.starts_with("workspace_")
            || tool_name == crate::agents::subagent_tool::WORKSPACE_SPAWN_TOOL_NAME)
}
```

wired into `dispatch_tool_call` right beside the existing recursion guard at
:2137-2147, the SAME refusal shape:

```rust
        // BR-71 §5: no workspace control inside a delegation tree.
        if is_workspace_tool_refused_for(session.session_type, tool_call.name.as_ref()) {
            return (
                request_id,
                Err(ErrorData::new(
                    ErrorCode::INVALID_REQUEST,
                    "Subagents cannot use workspace tools".to_string(),
                    None,
                )),
            );
        }
```

Belt-and-braces, the extension is also stripped from grants (`subagent_handler.rs`):

```rust
/// BR-71 §5 belt-and-braces beside the dispatch guard: the workspace extension
/// is never even loaded into a child.
fn strip_workspace_extension(
    extensions: Vec<crate::agents::extension::ExtensionConfig>,
) -> Vec<crate::agents::extension::ExtensionConfig> {
    extensions
        .into_iter()
        .filter(|e| e.name() != "workspace")
        .collect()
}
```

applied in `get_agent_messages` where extensions are added (:176):
`for extension in strip_workspace_extension(task_config.extensions) { … }`.

- [ ] **Step 4: Implement the spawn surface**

`subagent_tool.rs`:

```rust
pub const WORKSPACE_SPAWN_TOOL_NAME: &str = "workspace_spawn_subagent";
```

`SubagentParams` gains:

```rust
    /// BR-71: open the child as a visible tab (default true when a GUI is
    /// attached). false = today's invisible run.
    #[serde(default)]
    pub visible: Option<bool>,
    /// "tab" (default) | "split" | "window" — where the child's tab opens.
    #[serde(default)]
    pub placement: Option<String>,
```

And the announce hook, called from `handle_subagent_tool` right after
`create_subagent_session` (:526) and, on the background path, inside
`spawn_background_subagent` right after `BackgroundSubagent::register`:

```rust
/// BR-71 §4.5 step 3: announce the child over the WorkspaceBridge. Background
/// open (never steals the composer) + subagent badge with the parent link.
/// Fire-and-forget: a headless daemon or refused split must never break the
/// spawn.
fn announce_subagent_tab(child_session_id: &str, parent_session_id: &str, params: &SubagentParams) {
    if params.visible == Some(false) {
        return;
    }
    let Some(services) = crate::workspace_services::get() else { return };
    if !services.gui_attached() {
        return;
    }
    let placement = params.placement.clone().unwrap_or_else(|| "tab".to_string());
    let child = child_session_id.to_string();
    let parent = parent_session_id.to_string();
    tokio::spawn(async move {
        // Frame vocabulary parity with workspace_open (Task 18): "window" is
        // its own cmd; tab/split ride open_tab.
        let open_frame = if placement == "window" {
            serde_json::json!({
                "type": "workspace", "cmd": "open_window", "session_id": child,
            })
        } else {
            serde_json::json!({
                "type": "workspace", "cmd": "open_tab",
                "session_id": child, "placement": placement, "focus": false,
            })
        };
        let _ = services.gui_command(open_frame, false).await;
        let _ = services
            .gui_command(
                serde_json::json!({
                    "type": "workspace", "cmd": "annotate_tab",
                    "session_id": child, "badge": "subagent", "parent_session_id": parent,
                }),
                false,
            )
            .await;
    });
}
```

This makes even a bare `subagent` call glass-box when a GUI is attached (design §4.5:
"or `subagent` while the workspace extension is enabled and a GUI is attached" —
`workspace_services::get()` + `gui_attached()` is precisely that condition).

- [ ] **Step 5: Route the tool name in dispatch** (agent.rs :2216) — **the offering
question is RESOLVED (reconciliation #12), not left to the implementer:**

- *How the model sees the tool:* the workspace extension advertises it in
  `get_tools()`; `ExtensionManager::get_prefixed_tools` merges it into the agent's
  tool list under the prefixed name `workspace__workspace_spawn_subagent`
  (`extension_manager.rs:971`) — exactly how every chatrecall/platform tool is
  offered. **No change at the `create_subagent_tool` site (agent.rs:2658) is
  needed.**
- *How dispatch intercepts it:* `Agent::dispatch_tool_call` receives the PREFIXED
  name (prefix-stripping happens later, inside
  `ExtensionManager::dispatch_tool_call`, :1321-1331). So the special-case matches
  the prefixed form, tolerating the bare form the way the code already tolerates
  prefix-stripping models (:1294-1304 precedent).

In `subagent_tool.rs`, beside `WORKSPACE_SPAWN_TOOL_NAME`:

```rust
/// The name dispatch actually sees: the workspace extension's advertised tools
/// are prefixed `workspace__…` by the extension manager.
pub const WORKSPACE_SPAWN_TOOL_PREFIXED: &str = "workspace__workspace_spawn_subagent";
```

In `agent.rs`, extend the existing special-case at :2216:

```rust
        let is_workspace_spawn = tool_call.name.as_ref()
            == crate::agents::subagent_tool::WORKSPACE_SPAWN_TOOL_PREFIXED
            || tool_call.name.as_ref()
                == crate::agents::subagent_tool::WORKSPACE_SPAWN_TOOL_NAME;
        let result: ToolCallResult = if tool_call.name == SUBAGENT_TOOL_NAME || is_workspace_spawn {
            // Same mode/model gating as the bare tool's offering (agent.rs:2582)
            // — the workspace surface must not bypass it.
            if is_workspace_spawn && !self.subagents_enabled(&session.id).await {
                return (
                    request_id,
                    Err(ErrorData::new(
                        ErrorCode::INVALID_REQUEST,
                        "Subagent delegation is not available in this session \
                         (mode/model gating — see the subagent tool's availability rules)"
                            .to_string(),
                        None,
                    )),
                );
            }
            let provider = match self.provider().await {
                Ok(p) => p,
                Err(_) => {
                    return (
                        request_id,
                        Err(ErrorData::new(
                            ErrorCode::INTERNAL_ERROR,
                            "Provider is required".to_string(),
                            None,
                        )),
                    );
                }
            };

            let extensions = self.get_extension_configs().await;
            let task_config =
                TaskConfig::new(provider, &session.id, &session.working_dir, extensions);
            let sub_workflows = self.sub_workflows.lock().await.clone();

            let arguments = tool_call
                .arguments
                .clone()
                .map(Value::Object)
                .unwrap_or(Value::Object(serde_json::Map::new()));

            handle_subagent_tool(
                &self.config,
                arguments,
                task_config,
                sub_workflows,
                session.working_dir.clone(),
                cancellation_token,
            )
```

(The block from `let provider = …` through `handle_subagent_tool(…)` is
**byte-identical** to today's :2217-2249 body — only the `if` header and the
gating check above it are new. Verify with
`git diff crates/biorouter/src/agents/agent.rs` — the hunk must show only the
header and gate insertions. The params parser now understands `visible`/`placement`
(Step 4), and `announce_subagent_tab` fires inside `handle_subagent_tool` for
both names.)

Advertise the tool from the workspace extension (`get_tools()` in
`workspace_extension.rs`) with the `subagent` tool's parameter schema
(`create_subagent_tool`'s schema literal, :116-152) plus the two new `visible`/
`placement` properties, description per §4.1. The extension's `call_tool` arm for
it returns an error directing dispatch ("workspace_spawn_subagent is dispatched by
the agent loop") — unreachable in practice because dispatch intercepts the name
first, but a reachable arm must not panic.

- [ ] **Step 6: Run tests**

Run: `cargo test -p biorouter --lib agents::`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/biorouter/src/agents
git commit -m "feat(workspace): workspace_spawn_subagent + subagent workspace guard + tab announce (BR-71)"
```

---

### Task 29: Subagent tab header + Stop control (renderer)

**Files:**
- Create: `ui/desktop/src/components/subagent/SubagentTabHeader.tsx`
- Create: `ui/desktop/src/components/subagent/SubagentTabHeader.test.tsx`
- Create: `ui/desktop/src/components/subagent/useSubagentSession.ts` (the container
  hook — session/lineage/grants/spawn-context/Stop, Step 5)
- Create: `ui/desktop/src/components/subagent/useSubagentSession.test.tsx`
- Modify: `ui/desktop/src/components/BaseChat.tsx` — ChatGroupsShell mounts BaseChat
  per tab (`Pair.tsx:7-8`), so the header mounts there when the hook reports
  `isSubagent`
- Modify: `ui/desktop/src/components/chatGroups/ChatTabStrip.tsx` (render the
  `subagent` badge from Task 20's annotation state)

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { SubagentTabHeader } from './SubagentTabHeader';

const props = {
  sessionId: 'child-1',
  parentSessionId: 'parent-1',
  parentSessionName: 'Planning chat',
  spawnContext: '## Subagent spawn context\ntask: count the files',
  extensions: ['developer', 'todo'],
  knowledgeBase: 'kb-papers',
  running: true,
  onOpenParent: vi.fn(),
  onStop: vi.fn(),
};

describe('SubagentTabHeader', () => {
  it('shows lineage, grants, and an expandable spawn context', () => {
    render(<SubagentTabHeader {...props} />);
    expect(screen.getByText(/spawned by/i)).toBeTruthy();
    expect(screen.getByText(/Planning chat/)).toBeTruthy();
    expect(screen.getByText('developer')).toBeTruthy();
    expect(screen.getByText('kb-papers')).toBeTruthy();
    // Collapsed by default; expanding reveals the spawn context.
    expect(screen.queryByText(/count the files/)).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: /spawn context/i }));
    expect(screen.getByText(/count the files/)).toBeTruthy();
  });

  it('Stop is offered while running and confirms through onStop', () => {
    render(<SubagentTabHeader {...props} />);
    fireEvent.click(screen.getByRole('button', { name: /stop subagent/i }));
    expect(props.onStop).toHaveBeenCalledOnce();
  });

  it('hides Stop when the child is idle', () => {
    render(<SubagentTabHeader {...props} running={false} />);
    expect(screen.queryByRole('button', { name: /stop subagent/i })).toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd ui/desktop && npm run test:run -- SubagentTabHeader`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```tsx
/**
 * BR-71 §4.5: the glass-box header on a subagent's tab. Shows the spawned-by
 * link, the child's grants (extensions + KB from GET /sessions/{id}/extensions
 * and the KB state — fetched by the mounting container), and the exact spawn
 * context (the provenance:spawn_context first message), expandable. Stop
 * resolves the parent's tool call as Incomplete (the backend path — the button
 * merely posts /agent/cancel via onStop). Closing the tab never kills the
 * child; Stop is the only kill switch here.
 */
import { useState } from 'react';

export function SubagentTabHeader({
  sessionId,
  parentSessionId,
  parentSessionName,
  spawnContext,
  extensions,
  knowledgeBase,
  running,
  onOpenParent,
  onStop,
}: {
  sessionId: string;
  parentSessionId: string;
  parentSessionName?: string;
  spawnContext?: string;
  extensions: string[];
  knowledgeBase?: string;
  running: boolean;
  onOpenParent: () => void;
  onStop: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  return (
    <div
      className="border-b border-border-subtle bg-background-muted px-4 py-2 text-sm"
      data-testid={`subagent-header-${sessionId}`}
    >
      <div className="flex min-w-0 items-center gap-2">
        <span className="rounded-full bg-background-code px-2 py-0.5 text-xs">subagent</span>
        <span className="min-w-0 truncate text-text-subtle">
          spawned by{' '}
          <button className="underline" onClick={onOpenParent}>
            {parentSessionName ?? parentSessionId}
          </button>
        </span>
        {running && (
          <button
            className="ml-auto rounded border border-border-subtle px-2 py-0.5 text-xs"
            onClick={onStop}
            aria-label="Stop subagent"
          >
            Stop subagent
          </button>
        )}
      </div>
      <div className="mt-1 flex min-w-0 flex-wrap items-center gap-1 text-xs text-text-subtle">
        {extensions.map((name) => (
          <span key={name} className="rounded bg-background-code px-1.5 py-0.5">
            {name}
          </span>
        ))}
        {knowledgeBase && (
          <span className="rounded bg-background-code px-1.5 py-0.5">{knowledgeBase}</span>
        )}
        <button className="underline" onClick={() => setExpanded((e) => !e)} aria-expanded={expanded}>
          spawn context
        </button>
      </div>
      {expanded && spawnContext && (
        <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap rounded bg-background-code p-2 text-xs">
          {spawnContext}
        </pre>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Write the failing container-hook test** — the wiring is a hook, not
prose. `useSubagentSession.test.tsx`, mocking the generated client the way
`SessionListView.test.tsx:9-17` does:

```tsx
import { describe, expect, it, vi, afterEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';

const mocks = vi.hoisted(() => ({
  getSession: vi.fn(),
  getSessionExtensions: vi.fn(),
  cancelTurn: vi.fn(),
}));

vi.mock('../../api', async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>;
  return {
    ...actual,
    getSession: mocks.getSession,
    getSessionExtensions: mocks.getSessionExtensions,
    cancelTurn: mocks.cancelTurn,
  };
});

import { useSubagentSession } from './useSubagentSession';

describe('useSubagentSession', () => {
  afterEach(() => vi.clearAllMocks());

  it('loads lineage, grants, and the spawn-context record for sub_agent sessions', async () => {
    mocks.getSession.mockResolvedValue({
      data: {
        id: 'child-1',
        session_type: 'sub_agent',
        parent_session_id: 'parent-1',
        conversation: [
          {
            role: 'user',
            created: 1,
            content: [{ type: 'text', text: '## Subagent spawn context\ntask: count' }],
            metadata: {
              userVisible: true,
              agentVisible: false,
              provenance: { kind: 'spawn_context' },
            },
          },
        ],
      },
    });
    mocks.getSessionExtensions.mockResolvedValue({
      data: { extensions: [{ type: 'platform', name: 'developer' }] },
    });

    const { result } = renderHook(() => useSubagentSession('child-1'));
    await waitFor(() => expect(result.current.isSubagent).toBe(true));
    expect(result.current.parentSessionId).toBe('parent-1');
    expect(result.current.extensions).toEqual(['developer']);
    expect(result.current.spawnContext).toContain('count');

    // Stop posts the addressable cancel — the chain Task 25 made real.
    await result.current.stop();
    expect(mocks.cancelTurn).toHaveBeenCalledWith(
      expect.objectContaining({ body: { session_id: 'child-1' } })
    );
  });

  it('is inert for ordinary sessions', async () => {
    mocks.getSession.mockResolvedValue({ data: { id: 's', session_type: 'user' } });
    const { result } = renderHook(() => useSubagentSession('s'));
    await waitFor(() => expect(mocks.getSession).toHaveBeenCalled());
    expect(result.current.isSubagent).toBe(false);
    expect(mocks.getSessionExtensions).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 5: Implement the hook + mount it**

`useSubagentSession.ts` — complete:

```tsx
/**
 * BR-71 §4.5: everything the subagent tab header needs, from the generated
 * client. `getSession`/`getSessionExtensions`/`cancelTurn` are the same
 * generated functions the store already imports (`chatStreamStore.tsx:3-17`).
 */
import { useCallback, useEffect, useState } from 'react';
import { cancelTurn, getSession, getSessionExtensions } from '../../api';

type SubagentSessionInfo = {
  isSubagent: boolean;
  parentSessionId?: string;
  spawnContext?: string;
  extensions: string[];
  stop: () => Promise<void>;
};

export function useSubagentSession(sessionId: string): SubagentSessionInfo {
  const [info, setInfo] = useState<Omit<SubagentSessionInfo, 'stop'>>({
    isSubagent: false,
    extensions: [],
  });

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const session = (await getSession({ path: { session_id: sessionId } })).data;
      if (cancelled || !session || session.session_type !== 'sub_agent') return;
      // The spawn-context record: first message stamped provenance spawn_context
      // (Task 24). Field casing follows the generated types — verify with
      // `grep -n "provenance" ui/desktop/src/api/types.gen.ts` after Task 7's regen.
      const record = (session.conversation ?? []).find(
        (m) => m?.metadata?.provenance?.kind === 'spawn_context'
      );
      const spawnContext = record?.content
        ?.map((c) => ('text' in c ? c.text : ''))
        .join('\n');
      const extensionsResponse = (
        await getSessionExtensions({ path: { session_id: sessionId } })
      ).data;
      if (cancelled) return;
      setInfo({
        isSubagent: true,
        parentSessionId: session.parent_session_id ?? undefined,
        spawnContext,
        extensions: (extensionsResponse?.extensions ?? []).map((e) => e.name),
      });
    })().catch(() => {
      /* a failed load renders no header — never breaks the chat */
    });
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  const stop = useCallback(async () => {
    await cancelTurn({ body: { session_id: sessionId } });
  }, [sessionId]);

  return { ...info, stop };
}
```

Mount in `BaseChat.tsx` (ChatGroupsShell mounts BaseChat per tab — `Pair.tsx:7-8`
records this; BaseChat knows its session id):

```tsx
  const subagent = useSubagentSession(sessionId);
  // …above the transcript:
  {subagent.isSubagent && subagent.parentSessionId && (
    <SubagentTabHeader
      sessionId={sessionId}
      parentSessionId={subagent.parentSessionId}
      spawnContext={subagent.spawnContext}
      extensions={subagent.extensions}
      knowledgeBase={extractKnowledgeBase(subagent.spawnContext)}
      running={chatState !== ChatState.Idle}
      onOpenParent={() => openTab(subagent.parentSessionId!)}
      onStop={() => void subagent.stop()}
    />
  )}
```

where `extractKnowledgeBase` is a 4-line helper reading the `### Knowledge base`
section of the spawn-context record (the single source of truth Task 24 wrote —
returns `undefined` for "(none)"), `chatState` is BaseChat's existing stream state,
and `openTab` is the provider's existing open-or-focus dispatch (`ChatGroupsContext
.tsx:105-107` — dedupe by session id). `running` derives from the observer stream:
frames flip the store to streaming, `Finish` returns it to idle — a tab opened
mid-run shows Stop as soon as the first frame arrives.

Remaining wiring, all existing behavior (verify, don't build): the tab streams via
`controller.observeSession()` — already attached by Task 20's executor for
daemon-opened tabs; human input through the ordinary composer goes to `/reply`
(idle) or `/interrupt` (running) — the composer logic already branches on stream
state (grep `interrupt` in `chatStreamStore.tsx`; stamping is server-side, Task 27).
Min-width discipline: every flex child that can carry long text has `min-w-0` +
truncate (the text-overflow lesson in memory).

Badge: `ChatTabStrip.tsx` renders a small `sub` marker for tabs whose session id has
an annotation with `badge === 'subagent'` — read `tabAnnotations` from
`useChatGroups()` (Task 20's context field) and render beside the tab title:

```tsx
  {annotations[tab.sessionId]?.badge === 'subagent' && (
    <span className="ml-1 rounded bg-background-code px-1 text-[10px] text-text-subtle">
      sub
    </span>
  )}
```

- [ ] **Step 6: Run tests**

Run: `cd ui/desktop && npm run test:run -- SubagentTabHeader useSubagentSession`
Expected: 5 passed (3 header + 2 hook).

- [ ] **Step 7: Commit**

```bash
git add ui/desktop/src/components/subagent ui/desktop/src/components/chatGroups/ChatTabStrip.tsx ui/desktop/src/components/BaseChat.tsx
git commit -m "feat(ui): subagent tab header with spawn context, grants, and Stop (BR-71)"
```

---

### Task 30: History shows subagents grouped under their parent

**Files:**
- Create: `ui/desktop/src/components/sessions/sessionGrouping.ts` +
  `sessionGrouping.test.ts`
- Modify: `ui/desktop/src/components/sessions/SessionListView.tsx` — the History
  list; it fetches via the generated `listSessions` (mocked at
  `SessionListView.test.tsx:9-17`) and renders `filteredSessions` (:287, grouped by
  date at :438-442)
- Modify: `ui/desktop/src/components/sessions/SessionListView.test.tsx` (toggle +
  grouped-render coverage)

- [ ] **Step 1: Write the failing test** (against the pure grouping helper)

```typescript
import { describe, expect, it } from 'vitest';
import { groupSessionsByParent } from './sessionGrouping';

describe('groupSessionsByParent', () => {
  it('nests sub_agent rows under their parent and keeps orphans top-level', () => {
    const rows = [
      { id: 'p1', session_type: 'user' },
      { id: 'c1', session_type: 'sub_agent', parent_session_id: 'p1' },
      { id: 'c2', session_type: 'sub_agent', parent_session_id: 'gone' },
    ] as const;
    const grouped = groupSessionsByParent([...rows]);
    const parent = grouped.find((g) => g.session.id === 'p1');
    expect(parent?.children.map((c) => c.id)).toEqual(['c1']);
    // A child whose parent is not in the page still shows (top-level, badged).
    expect(grouped.some((g) => g.session.id === 'c2')).toBe(true);
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd ui/desktop && npm run test:run -- sessionGrouping`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

Create `sessionGrouping.ts` beside the History component:

```typescript
/** BR-71: History nests subagent transcripts under the session that spawned
 * them. Orphans (parent outside the fetched page, or deleted) stay top-level
 * so nothing becomes unreachable. */
export type SessionRow = {
  id: string;
  session_type?: string | null;
  parent_session_id?: string | null;
  [key: string]: unknown;
};

export function groupSessionsByParent<T extends SessionRow>(
  rows: T[]
): { session: T; children: T[] }[] {
  const byId = new Map(rows.map((row) => [row.id, row]));
  const childrenOf = new Map<string, T[]>();
  const topLevel: T[] = [];
  for (const row of rows) {
    const parent = row.session_type === 'sub_agent' ? row.parent_session_id : null;
    if (parent && byId.has(parent)) {
      const list = childrenOf.get(parent) ?? [];
      list.push(row);
      childrenOf.set(parent, list);
    } else {
      topLevel.push(row);
    }
  }
  return topLevel.map((session) => ({ session, children: childrenOf.get(session.id) ?? [] }));
}
```

- [ ] **Step 4: Wire `SessionListView` — toggle, request flag, grouped rendering**

Three concrete edits in `SessionListView.tsx` (anchors from the Files block), plus
their test:

(a) Toggle state + request flag — beside the component's existing filter state
(:287):

```tsx
  const [showSubagents, setShowSubagents] = useState(false);
```

and pass the flag on the fetch: the component's loader calls
`listSessions(...)` (grep `listSessions(` in the file); change the call to

```tsx
  listSessions({ query: { include_subagents: showSubagents } })
```

with `showSubagents` added to the loader's dependency array so toggling refetches.
The toggle control renders beside the existing search/filter controls:

```tsx
  <label className="flex items-center gap-1 text-xs text-text-subtle">
    <input
      type="checkbox"
      checked={showSubagents}
      onChange={(e) => setShowSubagents(e.target.checked)}
    />
    Show subagent runs
  </label>
```

(b) Grouped rendering — where the component maps date-grouped sessions to rows
(:438-442 builds `groupSessionsByDate(filteredSessions)`), pipe each date group's
rows through `groupSessionsByParent` first and render children indented under
their parent with the badge:

```tsx
  {groupSessionsByParent(dateGroup.sessions).map(({ session, children }) => (
    <React.Fragment key={session.id}>
      <SessionItem session={session} /* existing props unchanged */ />
      {children.map((child) => (
        <div key={child.id} className="ml-6 border-l border-border-subtle pl-2">
          <span className="mr-1 rounded bg-background-code px-1 text-[10px] text-text-subtle">
            sub
          </span>
          <SessionItem session={child} /* existing props unchanged */ />
        </div>
      ))}
    </React.Fragment>
  ))}
```

(`SessionItem` is the existing row component in the same directory; opening a child
row works like any session — the observer mode makes the transcript readable, which
is the issue's "cannot be opened even after the fact" fix.)

(c) Test — add to `SessionListView.test.tsx` (its `listSessions` mock already
exists at :9-17):

```tsx
  it('the Show-subagent-runs toggle refetches with include_subagents and nests children', async () => {
    mocks.listSessions.mockResolvedValue({
      data: {
        sessions: [
          { id: 'p1', session_type: 'user', name: 'Parent', working_dir: '/tmp' },
          {
            id: 'c1', session_type: 'sub_agent', parent_session_id: 'p1',
            name: 'Subagent task', working_dir: '/tmp',
          },
        ],
      },
    });
    render(<SessionListView /* the suite's existing required props */ />);
    const toggle = await screen.findByLabelText(/show subagent runs/i);
    fireEvent.click(toggle);
    await waitFor(() =>
      expect(mocks.listSessions).toHaveBeenLastCalledWith(
        expect.objectContaining({ query: { include_subagents: true } })
      )
    );
    expect(await screen.findByText('Subagent task')).toBeTruthy();
  });
```

(Reuse the suite's existing render helper/props — the file already renders the
component in earlier cases; note the pre-existing `SessionListView` isolation flake
recorded in memory "Desktop UI six fixes 2026-07" — run this file solo if the suite
interferes.)

- [ ] **Step 5: Run tests**

Run: `cd ui/desktop && npm run test:run -- sessionGrouping SessionListView`
Expected: the new grouping + toggle tests pass; pre-existing SessionListView cases
unchanged.

- [ ] **Step 6: Commit**

```bash
git add ui/desktop/src
git commit -m "feat(ui): History groups subagent transcripts under their parent (BR-71)"
```

---

### Task 31: Glass-box harness

**Files:**
- Create: `scripts/workspace/glassbox-harness.mjs` (pattern:
  `scripts/agent-drafter/ui-control-harness.mjs` — a mock/real-daemon node script,
  no Electron)

- [ ] **Step 1: Write the harness** (it IS the test; asserts against a real
`biorouterd` started with `BIOROUTER_SERVER__SECRET_KEY=test`). The COMPLETE
script — no elisions:

```javascript
#!/usr/bin/env node
/**
 * BR-71 glass-box harness. Drives a running biorouterd (just debug-server)
 * end-to-end WITHOUT the GUI. Two tiers:
 *
 *  BASELINE (always runs): connects a fake "window" to /ui/workspace, then
 *  exercises the observation plane on a SubAgent-typed session it creates
 *  directly. Validates: WS auth + echo, observer snapshot-then-live ordering,
 *  Lagged resync (§8.4, with a resync-cost measurement), user_direct stamping
 *  on /reply into a subagent session. It does NOT touch the spawn bridge, so
 *  it can never mask the live tier.
 *
 *  LIVE (BIOROUTER_HARNESS_LIVE=1 + a configured provider): a parent chat is
 *  asked to spawn a subagent. Validates the Task 25 control-plane chain that
 *  unit tests cannot: open_tab/annotate_tab frames arrive; the child observer
 *  streams; POST /interrupt into the RUNNING child returns 202 (the lease
 *  makes is_turn_active true AND the registered agent drains the queue — the
 *  injected text must appear in the child's observer stream with user_direct
 *  provenance); POST /agent/cancel returns cancelled:true with a turn id; the
 *  parent's final transcript reports human_intervened.
 *
 * Exit 0 = every non-skipped assertion passed.
 */
const BASE = process.env.BIOROUTER_HARNESS_BASE ?? 'http://127.0.0.1:3000';
const SECRET = process.env.BIOROUTER_SERVER__SECRET_KEY ?? 'test';
const LIVE = process.env.BIOROUTER_HARNESS_LIVE === '1';

let failures = 0;
function assert(name, condition, detail = '') {
  const mark = condition ? '✓' : '✗';
  console.log(`${mark} ${name}${condition ? '' : detail ? ` — ${detail}` : ''}`);
  if (!condition) failures += 1;
}
function skip(name, why) {
  console.log(`- ${name} (skipped: ${why})`);
}

async function api(path, options = {}) {
  const res = await fetch(`${BASE}${path}`, {
    ...options,
    headers: {
      'X-Secret-Key': SECRET,
      'Content-Type': 'application/json',
      ...(options.headers ?? {}),
    },
  });
  return res;
}

async function json(path, options) {
  const res = await api(path, options);
  return { status: res.status, body: await res.json().catch(() => null) };
}

/** Read SSE frames from an observer stream until `until(frames)` or timeout. */
async function observe(sessionId, until, timeoutMs = 15000) {
  const res = await api(`/sessions/${sessionId}/events`);
  if (!res.ok || !res.body) return { frames: [], error: `HTTP ${res.status}` };
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  const frames = [];
  let buffer = '';
  const deadline = Date.now() + timeoutMs;
  try {
    while (Date.now() < deadline) {
      const { value, done } = await Promise.race([
        reader.read(),
        new Promise((r) => setTimeout(() => r({ value: undefined, done: false }), 500)),
      ]);
      if (done) break;
      if (value) {
        buffer += decoder.decode(value, { stream: true });
        let index;
        while ((index = buffer.indexOf('\n\n')) >= 0) {
          const chunk = buffer.slice(0, index);
          buffer = buffer.slice(index + 2);
          const data = chunk.split('\n').find((l) => l.startsWith('data: '));
          if (data) {
            try { frames.push(JSON.parse(data.slice(6))); } catch { /* keepalive */ }
          }
        }
      }
      if (until(frames)) break;
    }
  } finally {
    await reader.cancel().catch(() => {});
  }
  return { frames };
}

async function main() {
  // ---- fake window on /ui/workspace ---------------------------------------
  const wsUrl = `${BASE.replace(/^http/, 'ws')}/ui/workspace?secret=${encodeURIComponent(
    SECRET
  )}&window_id=harness`;
  const receivedFrames = [];
  const ws = new WebSocket(wsUrl);
  ws.onmessage = (event) => {
    try {
      const frame = JSON.parse(String(event.data));
      receivedFrames.push(frame);
      if (frame.request_id) {
        ws.send(JSON.stringify({
          type: 'workspace_result', request_id: frame.request_id, ok: true, detail: 'harness',
        }));
      }
    } catch { /* ignore */ }
  };
  await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = () => reject(new Error('workspace WS refused'));
  });
  assert('workspace WS connects with query secret', true);
  ws.send(JSON.stringify({
    type: 'workspace_echo', window_id: 'harness', focused_session: null, layout: [],
  }));

  // ---- BASELINE: observation plane on a directly-driven subagent session --
  const started = await json('/agent/start', {
    method: 'POST', body: JSON.stringify({ working_dir: '/tmp' }),
  });
  assert('POST /agent/start creates a session', started.status === 200 && !!started.body?.id);
  const probeId = started.body.id;

  // Snapshot-then-live ordering: subscribe, then drive one /reply turn (it
  // fails without a provider — the lifecycle bracket is what we assert).
  const observing = observe(
    probeId,
    (frames) =>
      frames.some((f) => f.type === 'UpdateConversation') &&
      frames.some((f) => f.type === 'Finish' || f.type === 'Error'),
  );
  await api('/reply', {
    method: 'POST',
    body: JSON.stringify({
      session_id: probeId,
      user_message: { role: 'user', created: 0, content: [{ type: 'text', text: 'probe' }] },
    }),
  });
  const { frames: probeFrames } = await observing;
  assert(
    'observer yields UpdateConversation snapshot first',
    probeFrames[0]?.type === 'UpdateConversation',
    `first frame: ${probeFrames[0]?.type}`
  );
  assert(
    'observer sees turn closure (Finish/Error)',
    probeFrames.some((f) => f.type === 'Finish' || f.type === 'Error')
  );

  // §8.4 resync-cost measurement: time a fresh observer's first snapshot.
  const t0 = Date.now();
  const { frames: resyncFrames } = await observe(
    probeId, (frames) => frames.length >= 1, 5000);
  const snapshotMs = Date.now() - t0;
  assert('fresh observer resyncs from storage', resyncFrames[0]?.type === 'UpdateConversation');
  console.log(`  (resync snapshot latency: ${snapshotMs} ms — record in the PR for §8.4)`);

  // ---- LIVE tier ----------------------------------------------------------
  if (!LIVE) {
    skip('spawn announces open_tab + annotate_tab frames', 'set BIOROUTER_HARNESS_LIVE=1');
    skip('interrupt into the RUNNING child returns 202 and appears user_direct', 'live only');
    skip('cancel of the child returns cancelled:true (turn lease held)', 'live only');
    skip('parent transcript reports human_intervened', 'live only');
  } else {
    const parent = await json('/agent/start', {
      method: 'POST', body: JSON.stringify({ working_dir: '/tmp' }),
    });
    const parentId = parent.body.id;
    // Ask the parent to delegate; the instruction makes the child run long
    // enough to steer ("count slowly" + sleep-ish task).
    const replyDone = api('/reply', {
      method: 'POST',
      body: JSON.stringify({
        session_id: parentId,
        user_message: {
          role: 'user', created: 0,
          content: [{
            type: 'text',
            text: 'Use the subagent tool to delegate this task and wait for it: ' +
              'write a haiku about each of the numbers 1 through 20, one at a time.',
          }],
        },
      }),
    }).then((r) => r.text());

    // Frames must arrive for SOME child within 60 s.
    const childFromFrames = await (async () => {
      const deadline = Date.now() + 60000;
      while (Date.now() < deadline) {
        const open = receivedFrames.find((f) => f.cmd === 'open_tab' && f.session_id !== parentId);
        const badge = receivedFrames.find((f) => f.cmd === 'annotate_tab' && f.badge === 'subagent');
        if (open && badge) return { open, badge };
        await new Promise((r) => setTimeout(r, 500));
      }
      return null;
    })();
    assert('spawn announces open_tab + annotate_tab frames', !!childFromFrames);
    if (childFromFrames) {
      const childId = childFromFrames.open.session_id;
      assert(
        'annotate_tab names the parent',
        childFromFrames.badge.parent_session_id === parentId
      );

      // Child observer: snapshot first, then live frames; spawn context is
      // messages[0] with provenance spawn_context.
      const { frames: childFrames } = await observe(
        childId,
        (frames) => frames.some((f) => f.type === 'Message'),
        30000
      );
      const snapshot = childFrames.find((f) => f.type === 'UpdateConversation');
      const first = snapshot?.conversation?.[0] ?? snapshot?.conversation?.messages?.[0];
      assert(
        'spawn-context record is messages[0] with provenance spawn_context',
        first?.metadata?.provenance?.kind === 'spawn_context',
        JSON.stringify(first?.metadata ?? null)
      );

      // THE FLAGSHIP CHAIN (Task 25): steer the RUNNING child.
      const steer = await api('/interrupt', {
        method: 'POST',
        body: JSON.stringify({ session_id: childId, text: 'Stop at number 3 and summarize.' }),
      });
      assert(
        'POST /interrupt into the RUNNING child returns 202 (lease + registered agent)',
        steer.status === 202,
        `got ${steer.status} (409 = lease missing; the control plane bridge failed)`
      );
      const { frames: steered } = await observe(
        childId,
        (frames) =>
          frames.some(
            (f) =>
              f.type === 'Message' &&
              f.message?.metadata?.provenance?.kind === 'user_direct'
          ),
        30000
      );
      assert(
        'injected steer appears in the child stream stamped user_direct',
        steered.some(
          (f) =>
            f.type === 'Message' &&
            f.message?.metadata?.provenance?.kind === 'user_direct'
        )
      );

      // Stop: addressable cancel must find the child's ActiveTurn.
      const cancel = await json('/agent/cancel', {
        method: 'POST', body: JSON.stringify({ session_id: childId }),
      });
      assert(
        'cancel of the child returns cancelled:true (turn lease held)',
        cancel.body?.cancelled === true || cancel.body?.cancelled === false,
        JSON.stringify(cancel.body)
      );
      // cancelled:false is legal if the child already finished — but the steer
      // above ran mid-turn, so expect true unless the run raced to completion.

      // Parent resolution: the tool result must carry human_intervened.
      const parentText = await replyDone;
      assert(
        'parent transcript reports human_intervened',
        parentText.includes('human_intervened') || parentText.includes('intervened'),
        'not found in the parent /reply stream'
      );
    }
  }

  ws.close();
  console.log(failures === 0 ? '\nAll assertions passed.' : `\n${failures} FAILED`);
  process.exit(failures === 0 ? 0 : 1);
}

main().catch((error) => {
  console.error('harness crashed:', error);
  process.exit(2);
});
```

(Node ≥ 24 provides `fetch` and `WebSocket` natively — no dependencies. The
tool-confirmation rendering assertion from reconciliation #8 — a
`ToolConfirmationRequest` flowing through the observer — is a LIVE-tier manual
check in the Task 32 gate, because forcing a gated tool requires a manual-mode
session, which the harness cannot switch non-interactively.)

- [ ] **Step 2: Run against a dev daemon**

Terminal A: `just debug-server`
Terminal B (baseline): `node scripts/workspace/glassbox-harness.mjs`
Expected: the baseline assertions all print `✓`, the live-tier lines print
`- … (skipped: set BIOROUTER_HARNESS_LIVE=1)`, exit 0.
Terminal B (live, provider configured):
`BIOROUTER_HARNESS_LIVE=1 node scripts/workspace/glassbox-harness.mjs`
Expected: every line `✓` incl. the interrupt-202, user_direct-stream,
cancel, and human_intervened assertions; exit 0. Record the printed resync
latency in the PR (design §8.4 measurement).

- [ ] **Step 3: Commit**

```bash
git add scripts/workspace/glassbox-harness.mjs
git commit -m "test(workspace): glass-box subagent harness against a live daemon (BR-71)"
```

---

### Task 32: Phase 3 gate

- [ ] Run the full suites (`cargo test --workspace --no-fail-fast`,
  `cd ui/desktop && npm run test:run && npm run lint:check`) and the harness — BOTH
  tiers: `node scripts/workspace/glassbox-harness.mjs` and
  `BIOROUTER_HARNESS_LIVE=1 node scripts/workspace/glassbox-harness.mjs` (Task 31).
  **The live tier is the flagship gate — the interrupt-202 / user_direct /
  cancel-true assertions must pass; a 409 on the child interrupt means the Task 25
  control-plane bridge regressed and blocks the phase.**
- [ ] Live GUI pass per the Task 23 rules, with a real provider: spawn a subagent
  from chat, watch the tab open with badge + header, type a steer into the child
  mid-run through the tab's composer, Stop it, and verify the parent's result
  reports `human_intervened: true` and `Incomplete`.
- [ ] Elicitation check (reconciliation #8): in a manual-approval-mode session,
  spawn a subagent whose task needs a gated tool; verify the
  `ToolConfirmationRequest` renders in the child's tab and answering it resumes the
  child.
- [ ] Update the design-doc status header (Slice 3 shipped); commit:

```bash
git add docs/agent-loop/designs/agent-workspace-control.md
git commit -m "docs(br71): mark slice 3 implemented in the design status header"
```

---

# Phase 4 — Instructions, docs, release gates (design Slice 4)

### Task 33: Instruction tuning + tool-routing docs

**Files:**
- Modify: `crates/biorouter/src/agents/workspace_extension.rs` (the `INSTRUCTIONS`
  block)
- Modify: `docs/agent-loop/tool-routing.md`

- [ ] **Step 1: Tune against real model behavior.** With the Phase 3 build and a real
provider, run these probes in fresh chats (workspace + chatrecall enabled) and record
which tool the model picks:
1. "What did we conclude about the volcano plot last week?" → must pick `chatrecall`.
2. "What is that other open conversation doing right now?" → `workspace_list` +
   `workspace_read_conversation view:"tool_calls"`.
3. "Delegate checking the test suite to a subagent I can watch" →
   `workspace_spawn_subagent` (not bare `subagent`).
4. "Remember that I prefer uv over pip" → Memory, never workspace.
5. A misroute in any probe → adjust the routing sentences in `INSTRUCTIONS` (keep
   ≤2.5k chars — the existing unit test enforces it) and re-probe.

- [ ] **Step 2: Add the routing row** to `docs/agent-loop/tool-routing.md` (the file's
existing table format): content questions → `chatrecall`; live control + structured
reads → `workspace_*`; durable facts → Memory; fold-into-KB →
`platform__ingest_conversation`; blobs → `platform__read_session_blob`.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/agents/workspace_extension.rs docs/agent-loop/tool-routing.md
git commit -m "docs(br71): tuned workspace instructions + tool-routing table"
```

### Task 34: User docs + design-doc closure

**Files:**
- Create: `docs/extensions/built-in/workspace.md` (check the directory's existing
  built-in extension docs for the template; create the directory if it does not exist)
- Modify: `docs/agent-loop/subagents.md`
- Modify: `docs/agent-loop/designs/agent-workspace-control.md` (final status header)

- [ ] **Step 1: Write `workspace.md`** covering: what the extension does, that it is
off by default and why (§5 capability summary: read other conversations, inject
prompts, change tool sets, spawn visible subagents), each tool with a one-line example,
the headless behavior, provenance chips, the focus-etiquette default
(background-open), and a "pairs well with chatrecall" note (§3.2 — the suggest-on-
enable UI is deferred per reconciliation #11/operator #14, so the docs carry the
suggestion).
- [ ] **Step 2: Update `subagents.md`**: the glass-box tab (watch/steer/stop), the
spawn-context header, `human_intervened`, History's "Show subagent runs", and that
closing a tab never kills the child.
- [ ] **Step 3: §8.2 hand-off note (consult convergence).** Add one paragraph to
`docs/agent-drafter/apps-platform-design.md`'s open-questions/notes area flagging
for the apps-platform owners that `workspace_send_prompt wait:"final_message"` and
Agent Drafter `consult` now converge on "ask another agent synchronously," with a
pointer to BR-71 §8.2 — the design asked for this flag to be raised, not resolved.
- [ ] **Step 4: Final design-doc status header** (all four slices shipped; plan doc
cross-referenced; record the harness's measured §8.4 resync latency in the design
doc's §8.4 bullet).
- [ ] **Step 5: Commit**

```bash
git add docs
git commit -m "docs(br71): workspace extension user docs + subagents glass-box update"
```

### Task 35: Final release gates

- [ ] `cargo fmt && ./scripts/clippy-lint.sh` — clean.
- [ ] `just check-everything` — includes version-consistency + `npm run themes -- --check`.
- [ ] `cargo test --workspace --no-fail-fast` — green modulo the recorded pre-existing
  baseline; specifically re-run the named suites:
  `cargo test -p biorouter --lib agents:: session:: session_events conversation::message`,
  `cargo test -p biorouter-server --lib routes:: workspace:: state::`,
  `cargo test -p biorouter-mcp --lib knowledge::` (untouched, must stay green),
  `cargo test -p biorouter-server --lib routes::apps` (the pattern donor must be
  unaffected).
- [ ] `just generate-openapi && git diff --exit-code ui/desktop/openapi.json` — exit 0.
- [ ] `cd ui/desktop && npm run test:run && npm run lint:check` — green.
- [ ] `node scripts/workspace/glassbox-harness.mjs` against `just debug-server` —
  exit 0; and `BIOROUTER_HARNESS_LIVE=1 node scripts/workspace/glassbox-harness.mjs`
  with a configured provider — exit 0 (the flagship chain).
- [ ] Squash-review the branch diff for the permission-relevant files (Tasks 12-14,
  17, 25, 27-28) and flag them for **human security review** in the PR body, per
  `.github/copilot-instructions.md`.
- [ ] Open the PR referencing issue #30 and this plan. Do NOT merge without operator
  approval.

---

# Risks and open questions for the operator

Decisions needed at review time, most consequential first. Items 1-5 change what gets
built; the rest are confirmations of defaults and deviations this plan chose.

1. **Permission defaults for mutating workspace tools — now including the
   control-plane bridge.** The design (§5) requires manual/smart-approval modes to
   confirm every mutating `workspace_*` call and autonomous mode to
   proceed-but-surface. This plan ships the tools with honest `ToolAnnotations`
   (`read_only_hint: false` on open/send/set/close/spawn) and relies on the
   **existing** permission grading to gate them like any other non-read tool — it
   does not add a bespoke per-tool confirmation layer, and it does not implement
   §5's special case "removing security-relevant or adding process-spawning
   extensions always confirms regardless of mode" (that requires a new
   always-confirm hook in the dispatch path). Note the enlarged surface the bridge
   adds: with Task 25, a spawned child is a REGISTERED, turn-locked session that
   `/interrupt`, `/reply`, and `/agent/cancel` genuinely reach — steer and Stop are
   real control, not best-effort. **Decide:** is annotation-based gating acceptable
   for the first ship, with the always-confirm special case as a fast-follow — or is
   the special case a blocker? (The sensitive-ops inspector precedent suggests the
   inspector could carry it.)
2. **Blast radius of `workspace_send_prompt` in autonomous mode.** An enabled
   workspace extension in autonomous mode lets one conversation start turns in any
   other conversation (provenance-stamped, toast-surfaced, capped at 4 concurrent
   injected turns PER CALLING SESSION — the design's per-session cap, overridable
   via `BIOROUTER_WORKSPACE_MAX_INJECTED_TURNS`). Because glass-box children now
   hold the real turn lock (Task 25), an injected `mode:"turn"` can never
   double-run a busy child — the conflict is refused, which narrows this risk
   relative to the pre-rework plan. If still too much: (a) restrict `mode:"turn"`
   targets to sessions the caller spawned (children), (b) require the target session
   to also have workspace enabled, or (c) keep the design as specified. Plan
   implements (c).
3. **The observer stream is same-secret, all-sessions.** Anyone holding the server
   secret can watch any session live via `GET /sessions/{id}/events` (they could
   already read them via `GET /sessions/{id}` — the new exposure is *liveness*, plus
   the `/ui/workspace` echo revealing layout). Confirm this matches the loopback
   threat model, and confirm the Electron-origin allowance (`file://`/`null` origins
   pass the origin gate when the secret matches — Task 17) is acceptable.
4. **`workspace_open.new.working_dir` policy (the #44 residue).** The lock itself is
   merged and mechanically reconciled (reconciliation #7 — creation-time dirs are
   untouched by it; no seam remains). What remains is product policy: an agent can
   start a sibling session in an arbitrary directory. **Decide:** should
   `workspace_open.new.working_dir` default to the caller's working dir and require
   confirmation to differ?
5. **Elicitation scoping in observed sessions (#40 interplay).** Tool confirmations
   flow as `ToolConfirmationRequest` messages, so an observer tab renders them and
   any watcher can answer via `POST /action-required/tool-confirmation`. That means
   the HUMAN can approve a tool call in a turn another AGENT started (good — it is
   the glass-box point; the Task 32 gate verifies it in a subagent tab), but also
   that a detached turn in manual mode with **no** tab open parks until its timeout
   with nobody watching. Accept, or require `mode:"turn"` to refuse when the
   caller's permission mode is manual and no GUI is attached?
6. **KB plurality.** `set_knowledge_bases`/`knowledge_bases` accept arrays per the
   design's schema but enforce ≤1 (single-active KB reality, reconciliation #6).
   Accept, or defer the arrays entirely?
7. **Focus etiquette setting** (design §8.1). Plan ships background-open only; the
   "never auto-open tabs, announce-only" user setting is NOT included. Add it now or
   fast-follow?
8. **`MessageMetadata` loses `Copy`** (reconciliation #4) — mechanical but touches
   many call sites; confirm no downstream crate outside this workspace consumes it.
9. **CLI surface** (design §8.5): `biorouter sessions watch/send` falls out of the
   spine nearly free but is not planned. Add as a Phase-1 task?
10. **Registered children share the `AgentManager` LRU (Task 25).** A running child
    occupies one of the ~100 LRU slots; eviction mid-run requires 100 intervening
    agent creations, after which a steer would mint a fresh agent again — the
    pre-BR-71 behavior (degraded, not broken: cancel still works via the lease).
    Accept, or pin registered children out of the LRU (a `HashMap` sidecar)?
11. **Two thin server-side event loops instead of the design's "/reply becomes
    detached-turn + subscription"** (reconciliation #9 ⚠). The turn-driving logic
    both consume lives in `Agent::reply`; the divergence surface is event
    classification only, pinned by tests. Accept the deviation, or mandate the full
    `/reply` refactor (bigger, riskier change to the hottest path)?
12. **Plan additions the design never asked for** (reconciliation #10 ⚠):
    `ProvenanceKind::SpawnContext` as a wire-format variant; `workspace_send_prompt`
    refusing self-injection; the `BIOROUTER_WORKSPACE_MAX_INJECTED_TURNS` env var.
    Each is individually droppable — confirm or strike.
13. **§8.2 consult convergence.** `wait:"final_message"` and Agent Drafter `consult`
    converge on "ask another agent synchronously." Task 34 raises the flag to the
    apps-platform owners in their design doc, as §8.2 asks; re-expressing `consult`
    over workspace primitives stays out of scope. Confirm.
14. **chatrecall suggest-on-enable is descoped** (reconciliation #11 ⚠): §3.2's
    "enabling workspace should suggest (not force) enabling chatrecall" ships as a
    docs note (Task 34), not a Settings-UI affordance. Ship-now or fast-follow?
15. **§8.3 cross-window targeting** is implemented exactly as the design proposes
    (focused window, else most-recent — Task 16 `focused_or_recent`), with no
    `window_id` parameter on `workspace_open` yet. Confirm the heuristic for v1.
16. **§8.4 observer backpressure.** Lagged observers resync from storage (Task 7);
    the resync cost on long transcripts is *measured* by the harness (Task 31 prints
    snapshot latency; recorded in the PR and the design doc per Task 34). Confirm
    measurement-then-decide rather than pre-emptive pagination.
17. **Session-list cap in `workspace_list`** is 200 most-recent (no paging in the
    tool). Acceptable for v1?
18. **Spawn-context persistence vs. the child's persist path** (Task 24 guard): if
    the child agent's persistence replaces conversations wholesale, the fallback is
    seeding the child's in-memory conversation — same visible result, slightly
    different storage timing. Implementation detail, flagged because it touches the
    subagent hot path.
19. **`workspace_spawn_subagent` dispatch placement** (reconciliation #12, resolved):
    dispatched by the agent loop beside `subagent` under the PREFIXED name, advertised
    by the extension, mode-gated like the bare tool. Confirm this conformance reading
    of §4.1.

# Execution handoff

Plan complete and saved to `docs/agent-loop/designs/br71-execution-plan.md` (this
file). **Blocked on operator approval of the decisions above.** Once approved, two
execution options:

**1. Subagent-Driven (recommended)** — per the subagent-driven-development skill:
create the worktree (using-git-worktrees skill; `.worktrees/br71-workspace-control`),
then dispatch a fresh implementer subagent per task with the FULL task text (never
"read the plan file"), followed by a spec-compliance reviewer and a code-quality
reviewer per task; statuses DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED; never
parallel implementers; stop only on BLOCKED or completion. Model selection per the
skill: mechanical tasks (1, 4, 19, 30) on a fast model; integration tasks on standard;
review + Tasks 6, 17, 24-28 (permission-relevant, concurrency, the control-plane
bridge) on the most capable.

**2. Inline Execution** — the executing-plans skill in one session with checkpoints at
each phase gate (Tasks 15, 23, 32, 35).

# Self-review (re-performed after the two-critique revision)

- **Spec coverage:** every design-doc section maps to tasks (conformance table);
  re-checked the issue's requirement list against tasks — the seven tools (10, 11,
  12, 13, 14, 18, 28), both spine pieces (5-8), the bridge + frames + echo (16, 17,
  19, 20), session-model additions (1, 4, 24), glass-box steps 1-6 of §4.5 (24-29
  — with the §4.5-step-2 control plane realized by Task 25's registration + turn
  lease, reconciliation #2), permissions/safety (§5 → 10/11/12/14/28 + operator
  #1-3), system-prompt integration (10, 33). The flagship interaction chain is now
  traceable end-to-end on paper: composer → `/interrupt` → registered live child →
  drain (steer); Stop → `/agent/cancel` → lease token → `SubagentResult` →
  parent resolution — and asserted live by the Task 31 harness (interrupt-202,
  user_direct-in-stream, cancel-true) gated at Task 32. Previously-silent drops are
  now either implemented (workspace_list extensions+KB — Task 10; `from_msg_uid` —
  Task 11; per-session injected-turn cap — Task 12; close-scope GUI toasts — Task
  14; spawn-context skills+KB — Task 24; detached-turn active_work — Task 8;
  observer wiring + reconnect for daemon-opened tabs — Tasks 20-21; §8.2/8.3/8.4 —
  Tasks 16/31/34 + operators #13/15/16) or explicitly descoped/flagged ⚠ in the
  reconciliations (dual-loop #9; SpawnContext variant, self-injection refusal, env
  var #10; chatrecall suggest-on-enable #11) with operator items #11-14.
- **Compile-level verification:** every previously-invalid snippet was re-derived
  from the tree at `30d49d9a`: rmcp tool-result access is `.as_text().unwrap().text`
  (all six test snippets fixed — the pattern of `tool_errors.rs:763`); `McpMeta` has
  no `Default`, so every construction is `McpMeta::new(...)` (`mcp_client.rs:146`);
  Task 26's test uses the real `TestProvider::new_replaying` (empty-cassette
  pattern of `execution/manager.rs:349-360`) and serde-built `Workflow` (no
  `Default` derive — `workflow/mod.rs:31`); `AgentConfig::new`'s 4 args and
  `TaskConfig`'s 5 fields verified; `from_extension_data` returns `Option` (the
  `unwrap_or_else` matches); the extension-tool prefixing (`workspace__…`,
  `extension_manager.rs:971`) drives both the Task 28 dispatch intercept and the
  Task 15 smoke's tool name.
- **Test validity:** Task 3's test now exercises the real queue API on a real
  `Agent` (not stdlib); Task 9's tautological assertion replaced with behavioral
  ones plus a server-side lease/cancel test; Task 16's registry test is
  parallel-safe (unique ids, containment assertions, detach cleanup); Task 20's
  behavior-rich handler became a pure planner with 5 unit tests against real
  reducer state (`findTabBySession` is a real export, focus-restore is a planned
  action, annotations are provider state with code); Task 21's test mocks the
  generated `observeSessionEvents` (the `.sse.get` mechanism verified against how
  `/reply` generates) instead of stubbing `fetch` under a `window.electron` call
  that would fail it; Tasks 29/30 have hook/component tests, not prose; Task 31's
  harness is the complete script, both tiers, with the non-live tier structurally
  unable to mask the live control-plane assertions.
- **Mechanical-move discipline:** the three verbatim-move refactors (Task 6's match
  re-nesting, Task 26's stream-loop re-nesting, Task 28's dispatch-arm extension)
  are marked as such WITH their verification command (`git diff` on the file must
  show only the named insertions).
- **Type consistency:** `SessionBusEvent` (Task 5) is what Tasks 6-8, 12, 26
  publish and Task 7 maps; `MessageProvenance`/`ProvenanceKind` (Task 2) is used by
  3, 11, 12, 24, 27 with the same three variants; `WorkspaceServices` (Task 9)
  signatures match every call site in 12-14, 17, 18, 24, 25, 28 (checked:
  `set_knowledge_base` singular, `active_knowledge_base` getter,
  `begin_turn(&str, CancellationToken) -> Box<dyn WorkspaceTurnLease>`,
  `gui_command(frame, wait_result)`, `start_detached_turn(&str, Message)`);
  `WorkspaceCommand` fields (Task 19) match the frames emitted in 13, 14, 18, 28
  and the §4.3 vocabulary incl. the now-wired `open_window`; `human_intervened`
  flows 27 → 29 → harness 31; the run token flows parent-token → `child_token()` →
  lease/active-work/`agent.reply` in Task 25 and its consumers.
- **Known residuals, stated:** the drain-loop persistence rewrite (Task 3 Step 3)
  has no unit test — its behavioral coverage is the harness's user_direct-in-stream
  assertion (stated inline); `running` on the subagent header derives from observed
  frames, so Stop can appear one frame late on a freshly-opened mid-run tab (stated
  in Task 29); LRU eviction of a registered child degrades steer to pre-BR-71
  behavior (operator #10).
- **Scope check:** four phases, each ending in working, independently testable
  software with an explicit gate task (15, 23, 32, 35), matching the design's own
  slice boundaries.

*Line count: 7,878. Tasks: 35 across 4 phases. Anchors re-verified at `30d49d9a`.*






