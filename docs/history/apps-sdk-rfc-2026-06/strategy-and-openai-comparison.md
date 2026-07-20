# BioRouter App SDK strategy RFC and OpenAI comparison (June 2026)

> **What this is.** The strategy RFC that benchmarked OpenAI's Agents SDK against
> BioRouter's existing primitives and proposed a layered App SDK — files, databases,
> orchestration, vault, sandboxed compute, guardrails, context — with a phased roadmap and
> a list of open decisions for the maintainer.
> **Status:** Historical record — authored 2026-06-23, and superseded by the 2026-07-12
> [Apps SDK v2 design spec](../../apps-sdk/v2-design.md) plus the code that shipped: the
> `biorouter-sandbox` crate now exists, and `agent_drafter` now carries `manifest.rs`,
> `vault.rs`, `control.rs` and `declare.rs`. The "open decisions" below were settled by
> what was built; the OpenAI comparison is a snapshot of a competitor SDK on that date.
> **Audience:** maintainers and developers working on the Agent Drafter / Apps SDK who want
> the reasoning behind the shipped design.

**Document date: 2026-06-23.** It is the "why and what" half of a pair; the code-level
"how" is [the implementation design RFC](implementation-design.md), authored one day later,
which turns this inventory into concrete Rust types, WebSocket frames and hook points. Read
that one for the implementation; read this one for the competitive framing, the seven
capability areas, and the ADOPT/CONSIDER/SKIP inventory.

> **Warning.** Figures in this document describe the tree as of 2026-06-23 and some were
> already stale a day later — see the inline notes on session schema version and the Auto
> Visualiser tool count. The OpenAI product descriptions in the announcement-level rows
> were never directly verified (see §1.4 and Appendix B).

## Terms used here

| Term | Meaning |
|---|---|
| **BRSDK** | BioRouter App SDK — the proposed client library (`templates/sdk.ts`) plus the server-side runner (`routes/apps.rs`) that a generated app talks to. |
| **HITL** | Human-in-the-loop: the agent pauses before a sensitive tool call and waits for a person to approve or decline it. |
| **KB** | Knowledge base — BioRouter's markdown + git + BM25 personal knowledge store (`crates/biorouter-mcp/src/knowledge/`). |
| **PII / PHI** | Personally identifiable information / protected health information — the two categories of sensitive content the guardrail work in §11.3 targets. |
| **`RunState`** | OpenAI's serializable snapshot of a paused agent run (pending tool calls, approvals, turn count) that can be written to disk and resumed, even in another process. |
| **Tripwire** | A guardrail outcome that fires when a check fails, blocking the tool call or failing the turn. |
| **AgentKit** | OpenAI's hosted product tier announced October 2025 (Agent Builder, ChatKit, Connector Registry, Evals). |
| **ChatKit** | OpenAI's embeddable, themeable chat UI component. |
| **RFT** | Reinforcement fine-tuning — OpenAI's hosted model-tuning offering. |

---

## 0. Summary

**The key insight:** BioRouter already owns, *inside the `biorouter` crate*, almost
every primitive OpenAI exposes through its SDK — a bounded agent loop
(`agent.rs`, `DEFAULT_MAX_TURNS`), Stop/goal hooks (`hooks/`, `agents/goal.rs`),
guardrail inspectors (`security/`, `permission/`, `tool_inspection.rs`), a
bounded sub-agent loop (`knowledge/subagent/loop_.rs`), automatic compaction at
80% (`context_mgmt/`), session persistence + resume (`session/session_manager.rs`),
and rich tools (Developer + Computer Controller MCP). What the Agent Drafter
**App SDK** (`agent_drafter/templates/sdk.ts`, ~8 public functions) exposes today
is a thin slice: *chat over a per-app WebSocket*. Generated apps cannot read
files, query data, define tools, run isolated compute, declare guardrails, or
control context.

**So the plan is mostly "surface + harden + standardize," not "build new
engines."** We lift BioRouter's internal capabilities into a coherent, layered,
provider-agnostic SDK whose shape borrows OpenAI's proven vocabulary
(Agent / Runner / Sessions / Handoffs / Guardrails / Tools / Tracing, plus
Sandbox / Manifest / Capabilities / Memory) **without** giving up BioRouter's
biggest advantage: it runs against *any* provider the user configures.

---

## 1. Side-by-side: OpenAI Agents SDK ↔ BioRouter

### 1.1 Primitive mapping

| OpenAI Agents SDK | BioRouter equivalent (today) | Status |
|---|---|---|
| `Agent` (instructions, model, tools, handoffs, outputType) | `Agent` + `SessionConfig` + per-app `AgentConfig` manifest (system_prompt, model, extensions, skills, KB, max_turns) | **Exists**, but no `outputType`/handoffs DSL |
| `Runner` / `run()` agent loop | `Agent::reply()` / `reply_internal()` bounded loop | **Exists** (`DEFAULT_MAX_TURNS=100`; app default 24) |
| `maxTurns` → `MaxTurnsExceededError` | `SessionConfig.max_turns` + `BIOROUTER_MAX_TURNS` | **Exists** |
| Tools: `tool()` / `@function_tool` | MCP tools via `rmcp` `#[tool]`/`#[tool_router]`; Developer/ComputerController/Knowledge servers | **Exists** (server-side), **not app-definable** |
| Hosted tools (file_search, code_interpreter, web_search) | `knowledge` BM25 search; `computercontroller` shell/scripts; `developer` shell/editor; `web_scrape` | **Exists** as MCP, different shape |
| MCP servers (stdio / streamable-http / hosted) | First-class: `extension_manager.rs` (stdio, HTTP, bundled) | **Exists**, strong parity |
| Handoffs (`transfer_to_*`) | `/goal` + sub-agent loop approximates; no handoff DSL | **Partial** |
| Agents-as-tools (`agent.asTool()`) | `knowledge` macros run a `SubAgent`; no generic primitive | **Partial** |
| Guardrails (input/output/tool, tripwires) | `ToolInspector` chain: `SecurityInspector` (malware patterns), `PermissionInspector`, repetition; Stop/goal hooks | **Exists**, different framing (inspectors, not declarative guardrails) |
| Human-in-the-loop (`needsApproval`, `interruptions`, `state.approve`, resume) | Permission modes (Approve/SmartApprove) block tool calls + ask user; `PermissionRequest` hook | **Exists** for *interactive* approval; **no serializable resume state** |
| Sessions / memory (`SQLiteSession`, `to_input_list`) | `session_manager.rs` SQLite (schema v7), conversation BLOB, resume, `fix_conversation` | **Exists**, strong |
| Compaction (`OpenAIResponsesCompactionSession`) | `context_mgmt/` auto-compaction at `DEFAULT_COMPACTION_THRESHOLD=0.8` + visibility levels | **Exists**, strong |
| `RunState` serialize/resume across processes | — | **Missing** (we persist sessions, not mid-run interruptible state) |
| Sandbox (`SandboxAgent`, `Manifest`, capabilities, providers, snapshots) | No isolation: tools run in-process / as subprocesses as the user, gated only by `.biorouterignore` + inspectors | **Missing** (security gap) |
| Sandbox Memory (lessons → `MEMORY.md`, progressive disclosure) | Knowledge base (markdown + git + BM25) is conceptually close but not wired as agent "memory" | **Adjacent** |
| Tracing (`withTrace`, spans, exporters) | Hooks (`PreToolUse`/`PostToolUse`/…), token counters, session token rollups | **Partial** (events, no trace tree/exporter) |
| Structured outputs (Zod / Pydantic `output_type`) | — (free-form text; chart blocks parsed heuristically) | **Missing** |
| Connector Registry (governed data/tool catalog) | BAAM marketplace + `registry.json` (extensions/skills) | **Adjacent** (catalog exists; not credential-governed) |
| ChatKit + Widgets (embeddable UI, charts/tables) | Agent Drafter App SDK `mountChat`, `renderMarkdown`, `renderChart`; autovisualiser `ui://` figures | **Exists**, arguably ahead for science viz |

> **Note.** The "schema v7" figure in the Sessions row was corrected the following day: the
> [implementation design RFC](implementation-design.md) records `CURRENT_SCHEMA_VERSION = 8`
> as of 2026-06-24, and the schema has advanced further since.

### 1.2 Similarities

- **Both are bounded tool-calling loops** over a model, with a max-turns cap,
  streaming, and MCP as the tool-extension substrate. The core execution model
  is the same shape.
- **Both separate model-visible history from runtime context** (OpenAI's
  `RunContext`; BioRouter's message visibility metadata `agent_only` /
  `user_visible` / `invisible`).
- **Both treat MCP as the integration backbone.** BioRouter is arguably *more*
  MCP-native (its built-in capabilities are themselves MCP servers).
- **Both have a marketplace/catalog** (Connector Registry vs BAAM `registry.json`).
- **Both have an embeddable chat UI with rich output** (ChatKit/Widgets vs the
  Agent Drafter App SDK + autovisualiser).

### 1.3 Differences (and where each is ahead)

**OpenAI is ahead on:**

- **Sandboxed, capability-scoped execution.** `SandboxAgent` + `Manifest` give a
  clean control-plane (agent loop, in trusted infra) vs compute-plane (isolated
  filesystem/shell, narrow creds) split, with pluggable backends
  (Unix-local, Docker, E2B, Modal, Daytona, Cloudflare, Vercel, Runloop) and
  snapshots. BioRouter runs everything **in-process as the user** — no isolation,
  no resource limits, no per-task credential scoping. *This is the single
  biggest gap.*
- **Declarative guardrails + serializable interruptible state.** Input/output/tool
  guardrails with tripwire exceptions, `needsApproval` → `interruptions` →
  `state.approve()` → resume `run(agent, state)` *in a different process*.
  BioRouter approves interactively but can't snapshot a paused run and resume it
  later/elsewhere.
- **Structured outputs as a contract** (`outputType`/`output_type`). BioRouter
  has none; it heuristically parses `chart` code blocks.
- **A formal trace tree + exporters.** BioRouter has hook events and token
  counters but no spanned trace or pluggable exporter.
- **Two-phase "sandbox memory"** that distills lessons into a searchable
  `MEMORY.md` with progressive disclosure.

**BioRouter is ahead on / differentiated by:**

- **Provider-agnosticism is the core, not an afterthought.** OpenAI's SDK is
  multi-provider-capable but OpenAI-centric (hosted tools, Responses API,
  Conversations API). BioRouter runs the *same* agent against Anthropic / OpenAI
  / Azure / Bedrock / Ollama / MiMo / llama.cpp. **Keep this as the headline.**
- **`/goal` loop with an LLM judge, stall detection, and graceful give-up**
  (`GOAL_MAX_ITERATIONS=20`, Jaccard stall detection) — a *goal-completion*
  harness OpenAI doesn't ship as a primitive.
- **Built-in scientific viz** (autovisualiser's 33 tools; chart blocks in the App
  SDK) tuned for biomedical work.
- **Knowledge base** (markdown + git history + BM25 + credibility classification)
  is a first-class, user-owned, versioned store — richer than a vector store.
- **Apps as a product**: the Agent Drafter already serves runnable apps, exports
  standalone runnable folders, and lists them in an Applications panel. OpenAI
  has Agent Builder (graph authoring) + ChatKit but not "export a portable app."

> **Note.** The Auto Visualiser tool count of 33 above is the figure as of 2026-06-23; the
> extension has since grown past it.

### 1.4 Sourcing caveats

OpenAI's own blog pages 403'd automated fetch; the *announcement-level* product
descriptions (AgentKit/Agent Builder/ChatKit/Connector Registry/Evals/RFT) are
from secondary coverage and should be re-confirmed in a browser before being
quoted. Everything in §1.1's *SDK* rows is grounded in the actual
`openai-agents-js` / `openai-agents-python` repos and `developers.openai.com`
docs (verbatim where it matters — e.g. the `SandboxAgent` example, the HITL
resume loop, the `src/agents/sandbox/` module tree). A few sandbox-*memory*
capability signatures (`memory()`, `shell()`, `filesystem()`) were summarized,
not read verbatim — verify against `examples/docs/sandbox-agents/` before
cloning them exactly.

---

## 2. Target architecture: the BioRouter App SDK (BRSDK)

Borrow OpenAI's **layering discipline** (`@openai/agents-core` engine +
provider packages + extensions) but apply it to what BioRouter already has.

```text
┌──────────────────────────────────────────────────────────────┐
│  Generated app (TypeScript, browser)                           │
│  import { createApp, defineTool, useFiles, useData, ... }      │
│        from "./sdk"     ← BRSDK client (templates/sdk.ts++)    │
└───────────────▲──────────────────────────────────────────────┘
                │  per-app WebSocket (typed frames)              
┌───────────────┴──────────────────────────────────────────────┐
│  biorouterd  routes/apps.rs   ← BRSDK server / "Runner"        │
│  • configure_agent (model+ext+skills+KB+persona+max_turns)     │
│  • NEW: capabilities, guardrails, manifest, run-state, trace   │
└───────────────▲──────────────────────────────────────────────┘
                │  in-process calls                              
┌───────────────┴──────────────────────────────────────────────┐
│  biorouter crate  ← the engine (ALREADY EXISTS)                │
│  agent.rs loop · hooks/ · agents/goal.rs · context_mgmt/ ·     │
│  security/ + permission/ · session_manager · knowledge ·       │
│  extension_manager · providers/ (any LLM)                      │
│  + NEW: sandbox/ (capability-scoped execution)                 │
└────────────────────────────────────────────────────────────────┘
```

Naming: keep the existing `br.*` client object; add capability namespaces
(`br.files`, `br.data`, `br.tools`, `br.workflow`, `br.context`) so the surface
grows without breaking the current `br.run/prompt/ask/on` API.

### 2.1 The App Manifest, extended

Today: `AgentConfig { system_prompt, greeting, model, extensions, skills,
knowledge_base, max_turns }` in `agent_drafter/store.rs`. Extend it to a
**workspace manifest** in the spirit of OpenAI's `Manifest`:

```jsonc
// manifest.json  → AgentConfig (extended)
{
  "system_prompt": "...", "greeting": "...",
  "model": null,                       // provider-agnostic (inherits user config)
  "extensions": ["developer", "knowledge"],
  "skills": ["scientific-research"],
  "knowledge_base": "spoke-kb",
  "max_turns": 24,

  // NEW — capabilities the app's agent is granted (deny-by-default)
  "capabilities": {
    "files":   { "root": "workspace", "mode": "rw", "max_bytes": 50000000 },
    "data":    { "sources": ["spoke", "duckdb:local"], "read_only": true },
    "compute": { "sandbox": "docker", "timeout_s": 120, "network": "none" },
    "tools":   ["summarize", "fetch_pubmed"],   // app-defined tool names
    "memory":  { "kb": "spoke-kb", "mode": "rw" }
  },

  // NEW — guardrails declared by the app author
  "guardrails": {
    "input":  [{ "name": "no_phi", "kind": "regex|llm|builtin" }],
    "output": [{ "name": "must_cite", "kind": "llm" }],
    "tools":  [{ "tool": "*", "needs_approval": ["delete_*", "shell"] }],
    "goal":   "Always return an answer grounded in the knowledge base."
  },

  // NEW — workspace mounts (Manifest entries), capability-scoped
  "workspace": {
    "entries": {
      "data": { "local_dir": "~/Desktop/project/data", "mode": "ro" },
      "out":  { "out_dir": true }
    },
    "vault": { "encrypted": ["secrets.env"] }   // see §3.4
  }
}
```

**Open decisions:**

- (a) Manifest format above as the single source of truth? (vs splitting
  guardrails/capabilities into separate files.)
- (b) Deny-by-default capabilities (recommended, OpenAI-style) vs the current
  "agent inherits everything the user can do" model.

---

## 3. The seven capability areas

For each: *what exists in BioRouter*, *how OpenAI does it*, *proposed BRSDK
design* (client TS API + server/engine work), and *open decisions*.

### 3.1 File access

**Exists:** `developer` MCP — `text_editor` (view/write/str_replace/insert/undo),
`shell` (with background jobs), `analyze`; `.biorouterignore` enforced via the
`ignore` crate (blocks reads/writes/shell touching ignored paths). All run as the
user. The browser app itself **cannot** touch files; it can only accept dropped
files and forward them to the agent.

**OpenAI:** filesystem is a *sandbox capability* (`Filesystem()` + `apply_patch`,
paths relative to the sandbox workspace root) declared in a `Manifest`
(`localDir`, `GitRepo`, `out_dir`, cloud mounts), plus `file_search` over hosted
vector stores for *retrieval*.

**Proposed BRSDK:**

- **Server**: a `files` capability backed by `developer`'s editor but **scoped to
  the app's workspace root** (`~/.config/biorouter/agent_drafter/<id>/workspace/`
  by default, or a manifest-mounted dir). The scope is a *hard* jail enforced in
  Rust (canonicalize + prefix check), layered on top of `.biorouterignore`.
- **Client** (`br.files`):

  ```ts
  br.files.list(glob?)            // → FileRef[]
  br.files.read(path)            // → string (text) | Blob
  br.files.write(path, content)  // gated by capability mode rw
  br.files.upload(File)          // browser file → workspace (replaces ad-hoc fileToImageInput for non-images)
  br.files.url(path)             // signed, short-lived GET under /apps/<id>/files/*
  ```

- **Retrieval**: expose the knowledge store's BM25 as `br.files.search(query)`
  for in-workspace search (distinct from KB search).
- **Manifest mounts**: `workspace.entries` → mount host dirs read-only/rw at
  app start (mirrors OpenAI `Manifest`).

**Open decisions:** workspace root default location; whether to allow
manifest mounts of arbitrary host dirs (powerful but a sandbox-escape vector —
see §3.5).

### 3.2 Databases

**Exists:** *internally* — `session_manager.rs` (SQLite, app-transparent),
`knowledge/store.rs` (markdown+git+BM25). *Externally* — SPOKE / OMOP / CDW are
**separate MCP extensions** (Cypher / OMOP-SQL / CDW). There is **no generic SQL
tool** and apps have no DB API.

**OpenAI:** no first-class DB tool either — structured data comes via
**Connectors** (governed MCP wrappers) + remote MCP + code-interpreter for
tabular analysis.

**Proposed BRSDK** (follow OpenAI: DB = governed MCP/connector, not a bespoke
layer):

- **`data` capability**: a manifest list of allowed sources. Each source is
  either (a) an existing MCP extension (SPOKE/OMOP/CDW/knowledge), or (b) a new
  **bundled lightweight SQL MCP** (DuckDB/SQLite over a workspace file) for
  app-local tabular work — the BioRouter analog of code-interpreter-for-data.
- **Client** (`br.data`):

  ```ts
  br.data.sources()                       // → ["spoke","duckdb:local",...]
  br.data.query(source, sql_or_cypher)    // → rows (read-only by default)
  br.data.table(source, name)             // → typed columns for rendering
  ```

- **Governance (Connector-Registry analog)**: extend `registry.json` / BAAM so a
  data source carries an auth scope + read-only flag + audit hook; the app can
  only name sources the *user* has authorized. Credentials stay in the keyring
  (§3.4), never in the app.

**Open decisions:** ship a bundled DuckDB SQL MCP for app-local data? Which
external DB agents to expose to apps by default (probably none — opt-in per app).

### 3.3 Agentic workflow orchestration

**Exists:** the real differentiator. `agent.rs` runs a full multi-tool loop per
message (already "workflow-like," bounded by `max_turns`). `knowledge/subagent/
loop_.rs` is a clean bounded sub-agent (`SubAgentBounds { max_steps:30,
max_wall:300s, max_tokens:200k }`, `Completer`/`ToolDispatch` traits, a
`complete` sentinel) — *exactly* the shape to generalize. `/goal`
(`agents/goal.rs`) is a goal-completion harness. No handoff DSL, no agents-as-
tools primitive, no structured-output contract.

**OpenAI:** two patterns only — **handoffs** (transfer ownership,
`transfer_to_*`) and **agents-as-tools** (`agent.asTool()`, manager keeps
ownership). Both are just fields on `Agent`. Plus `outputType` as the completion
contract and `RunState` for pause/resume.

**Proposed BRSDK:**

- **Generalize the sub-agent loop** out of `knowledge/` into a reusable
  `biorouter::agents::subagent` the App SDK can drive — this gives
  agents-as-tools for free.
- **Declarative multi-step workflows** in the manifest (steps with tool/agent +
  guardrail + on-error), executed server-side; the analog of Agent Builder
  graphs but **code-first + manifest-first**, not a visual canvas (a canvas can
  come later in the GUI).
- **Client** (`br.workflow`):

  ```ts
  br.workflow.run(name, input)            // run a manifest-defined workflow
  br.agent("summarizer").asTool()         // expose a sub-agent as a tool
  br.run(prompt, target, { outputType })  // structured-output contract (Zod-style schema)
  ```

- **Structured outputs**: add `output_type` (JSON Schema) to `SessionConfig`;
  validate the final message against it and re-prompt on mismatch (mirror
  OpenAI's strict mode). This makes `chart` code blocks a *typed contract*
  instead of heuristic parsing.
- **Handoffs**: optional later — model as "swap the active manifest agent."
  Lower priority than agents-as-tools + structured outputs.

**Open decisions:** how much workflow-graph authoring to expose to apps now
(manifest steps) vs defer to a GUI canvas; adopt JSON-Schema structured outputs
as a first-class contract (recommended).

### 3.4 Encryption / file sandboxing (secrets)

**Exists:** `config/base.rs` secret storage via the `keyring` crate (macOS
Keychain / Win Cred Manager / Linux Secret Service), cached in-process;
`BIOROUTER_DISABLE_KEYRING` → plaintext `secrets.yaml`; Windows chunking.
`.biorouterignore` protects files from being *read by the agent*. There is **no
per-app encrypted vault** and exported apps inherit the daemon's provider keys.

**OpenAI:** capability-scoped isolation ("an agent can only access what it's
explicitly given"); Connector Registry stores **encrypted credentials** with
RBAC + audit; sandbox manifests map Unix permissions.

**Proposed BRSDK:**

- **Per-app encrypted vault**: `workspace/.vault/` whose entries are encrypted at
  rest with a key from the OS keyring (reuse `config/base.rs`). Manifest
  `workspace.vault.encrypted` lists logical secret names. The agent gets secrets
  **by reference** (`{{vault:OPENAI_KEY}}`) resolved server-side at tool-call
  time — the plaintext never reaches the browser or the model context.
- **`.biorouterignore` per app**: auto-add `.vault/` and any `mode:"ro"` mounts'
  sensitive paths.
- **Export hardening**: exported apps reference the *user's* biorouterd
  credentials (already the design); the vault travels **encrypted** and is
  re-bound to the new machine's keyring on first run (`run.sh` prompts once).

**Open decisions:** vault crypto (recommend: OS-keyring-wrapped data key +
AES-GCM file encryption via an audited crate, not hand-rolled); whether secrets
are *ever* allowed into tool args vs always resolved by reference.

### 3.5 Sandboxed compute

**Exists:** `computercontroller` MCP — `shell`/`shell_wait`/`shell_kill`,
`automation_script` (Shell/Batch/Ruby/PowerShell), `computer_control`
(AppleScript/PowerShell), doc tools, `web_scrape`, `cache`. **All run as the
current user with no isolation, no resource limits, no network restriction.**
This is the **biggest security gap** vs OpenAI.

**OpenAI:** `SandboxAgent` + `RunConfig(sandbox=...)` — control-plane (agent
loop, trusted) vs compute-plane (isolated Unix-like FS+shell, narrow creds).
Pluggable `SandboxClient` backends: **`UnixLocalSandboxClient`**,
**`DockerSandboxClient`** in core; E2B / Modal / Daytona / Cloudflare / Vercel /
Runloop as extras. `Manifest` stages inputs; `Snapshot` (Local/Remote/Noop)
saves workspace; sessions reconnect.

**Proposed BRSDK** (adopt OpenAI's split almost verbatim):

- **New `biorouter::sandbox` module** with a `SandboxClient` trait:

  ```rust
  trait SandboxClient {
      async fn exec(&self, cmd, cwd, env, limits) -> ExecResult;
      async fn read/write/list(&self, path) -> ...;
      async fn snapshot(&self) -> SnapshotSpec;   // Local|Remote|Noop
  }
  ```

  Implementations, in priority order:
  1. **`LocalProcessSandbox`** (today's behavior) — *clearly labeled unsafe*, for
     trusted single-user desktop use; the migration target, not the end state.
  2. **`DockerSandbox`** — run `computercontroller`/`developer` tool calls inside
     a container with the manifest-mounted workspace, `--network none` by
     default, cgroup CPU/mem/pids limits, dropped caps, non-root user. This is
     the recommended default for any app that runs untrusted compute.
  3. **`RemoteSandbox`** (later) — E2B/Modal-style for heavy/cloud workloads.
- **Capability-gated**: `capabilities.compute.sandbox = "none|local|docker"`;
  apps default to `none` (no shell) and must opt in. `network`, `timeout_s`,
  `max_mem` from the manifest.
- **Client** (`br.compute`):

  ```ts
  br.compute.run(cmd, { timeout })   // only if capability granted
  br.compute.python(code)            // code-interpreter analog (Docker + python)
  ```

- **Snapshots** for long tasks → reuse for context recovery (§3.7).

**Open decisions:** Docker as the default isolation backend (needs Docker
present — fallback to `local` with a loud warning)? Or invest in a lighter OS
sandbox (macOS `sandbox-exec`, Linux namespaces/seccomp) to avoid the Docker
dependency? **Recommendation:** Docker first (portable, well-understood), native
OS-sandbox as a later optimization.

### 3.6 Agentic harness: guardrails + stop-hooks → always reach the goal

**Exists — and this is BioRouter's hidden strength:**

- **Hook system** (`hooks/`): `PreToolUse`, `PostToolUse`, `PostToolUseFailure`,
  `PermissionRequest`, `UserPromptSubmit`, **`Stop`** (block turn-exit),
  `SubagentStart/Stop`, `SessionStart/End`, `PreCompact/PostCompact`. Stop hook
  can `block` with feedback; a block cap prevents infinite loops.
- **Goal loop** (`agents/goal.rs`): `/goal` installs a Stop hook with an LLM
  judge; `GOAL_MAX_ITERATIONS=20` (doesn't reset on tool calls),
  `GOAL_STALL_LIMIT=3` via Jaccard similarity, truncation-aware judging,
  graceful give-up.
- **Inspector chain** (`tool_inspection.rs`): `SecurityInspector` (malware
  patterns), `PermissionInspector` (AlwaysAllow/AskBefore/NeverAllow),
  repetition; most-restrictive-wins.
- **Retry** (`agents/retry.rs`).

**OpenAI:** declarative input/output/**tool** guardrails returning
`GuardrailFunctionOutput { tripwire_triggered }`; per-tool `needsApproval` →
`interruptions` → `state.approve()` → resume; structured outputs as the
completion contract.

**Proposed BRSDK** — expose what exists as *app-declarable*, and add the missing
serializable resume:

- **Declarative guardrails in the manifest** (§2.1 `guardrails`), compiled to:
  - input/output guardrails → run an LLM-judge or regex/builtin check around the
    app's turn (reuse the goal-judge machinery);
  - tool guardrails → map to the existing inspector chain +
    `needs_approval` lists;
  - `guardrails.goal` → install the existing goal Stop-hook for the app session
    automatically (so "always reach the goal" is a one-line manifest field).
- **Tripwire semantics**: a triggered guardrail emits a typed
  `{type:"guardrail", name, blocked}` WS frame the app can render, and either
  blocks the tool or fails the turn (configurable), matching OpenAI's tripwire.
- **Serializable interruptions (the missing piece)**: when a tool `needs_approval`
  with no interactive UI (e.g., headless/exported app), snapshot the run to a
  `RunState` (new) persisted in the session DB; the app surfaces an approval UI;
  `br.approve(interruptionId)` resumes. This is OpenAI's
  `state.approve()/run(agent,state)` adapted to BioRouter's session store.
- **Client** (`br.on('guardrail'|'approval', …)`, `br.approve/reject(id)`).

**Open decisions:** (a) make `guardrails.goal` opt-in per app (recommended on
for "workflow" apps); (b) build the `RunState` snapshot/resume now (enables
headless approvals + crash recovery) vs defer.

### 3.7 Context: recovery, compaction, sharing

**Exists:**

- **Auto-compaction** (`context_mgmt/`): triggers at
  `DEFAULT_COMPACTION_THRESHOLD=0.8 × context_limit`; summarizes older messages;
  marks them `user_visible` but `agent_invisible`; inserts an `agent_only`
  summary + continuation text (variants for chat / tool-loop / manual). Token
  counting via tiktoken with a DashMap cache. `PreCompact/PostCompact` hooks.
- **Persistence/resume**: `session_manager.rs` stores the conversation (BLOB or
  ref) per session; `load_session` + `fix_conversation`/`debug_conversation_fix`
  repair malformed history on load.
- **Cross-agent**: sub-agents have separate conversations; their final text is
  fed back to the parent as a tool result.

**OpenAI:** `Session` strategies (`SQLiteSession`, `OpenAIConversationsSession`),
`OpenAIResponsesCompactionSession` family, `RunState` serialize/resume across
processes, and **sandbox memory** (lessons → searchable `MEMORY.md` with
progressive disclosure, two-phase distill).

**Proposed BRSDK:**

- **Expose compaction + recovery to apps** rather than keeping it transparent:

  ```ts
  br.context.tokens()                 // current usage vs limit
  br.context.compact()                // force a compaction (manual)
  br.on('compaction', e => …)         // PreCompact/PostCompact surfaced
  br.context.snapshot() / .restore(id)// RunState save/restore (crash recovery)
  ```

- **Durable per-app sessions**: today a browser disconnect can lose the
  per-connection session. Bind each app session to a stable
  `session_id = app_id + client_id`, persisted via `session_manager` so a reload
  *resumes* (OpenAI's resumable-session behavior). This is the "automatically
  recover what it lost" requirement.
- **RunState (new)**: serialize mid-run state (pending tool calls, approvals,
  turn count) to the session DB → crash/disconnect recovery + headless approvals
  (§3.6). Mirrors `RunState.fromString(agent, state)`.
- **Context sharing**: two scopes —
  1. *Within an app*: a shared "scratch memory" the manifest can enable
     (`capabilities.memory.kb`), backed by the **knowledge base** — this is
     BioRouter's natural analog of OpenAI sandbox memory (markdown + git + BM25 +
     progressive disclosure via `index.md`/`schema.md`). Add a two-phase
     "distill lessons after a session" job that writes to the KB.
  2. *Across apps/sessions*: opt-in shared KB id so multiple apps read/write a
     common knowledge store (cross-session transfer, which BioRouter lacks today).
- **Compaction quality**: optionally adopt OpenAI's progressive-disclosure idea —
  keep a compact running summary injected at the top + a searchable archive,
  instead of all-or-nothing compaction.

**Open decisions:** (a) make app sessions durable+resumable by default
(recommended); (b) use the knowledge base as the app-memory substrate
(recommended — reuses git/BM25/credibility) vs a new store; (c) build `RunState`
now (it underpins both recovery and headless approvals).

---

## 4. Cross-cutting: tracing and observability

**Exists:** hook events + per-session token rollups (`SessionTokenCounts`).
**OpenAI:** a spanned trace tree (`withTrace`, agent/generation/tool/guardrail
spans) + pluggable exporters.

**Proposed:** add a lightweight **trace tree** built from the existing hook
events (one span per turn / tool / sub-agent / guardrail), persisted per session
and streamable to the app as `{type:"trace", span}` frames — powering an
in-GUI "what did the agent do" timeline (an Evals/trace-grading foundation
later). Keep it provider-agnostic; no external exporter required initially.

**Open decision:** in-GUI trace timeline now, or defer until after the
capability work.

---

## 5. Proposed package / module layout

Mirror OpenAI's engine/provider/extensions split, but the engine already exists:

| Layer | Where | Work |
|---|---|---|
| **Engine** (loop, hooks, goal, context, sessions, security) | `crates/biorouter/src/` | mostly exists; add `sandbox/`, `RunState`, structured-output validation, generalized `subagent` |
| **App Runner** (per-app WS, capability wiring, manifest, guardrail compile) | `crates/biorouter-server/src/routes/apps.rs` | extend `configure_agent`; add capability/guardrail/runstate/trace handling + new routes (`/apps/<id>/files/*`, `/apps/<id>/approve`) |
| **App authoring** (tools) | `crates/biorouter-mcp/src/agent_drafter/` | extend manifest (`store.rs`), lint harness (`bundle.rs`) to validate new capability/guardrail declarations; add `configure_capabilities`/`define_workflow` tools |
| **App SDK (client)** | `agent_drafter/templates/sdk.ts` | add `br.files / br.data / br.compute / br.workflow / br.context`, `defineTool`, approvals, structured outputs, typed frames |
| **GUI** | `ui/desktop/src/components/applications/` | capability/guardrail editors; trace timeline; approval UI |

Keep `sdk.ts` **backward compatible** — current `br.run/prompt/ask/on/off`,
`renderMarkdown/renderChart`, `fileToImageInput`, `mountChat` stay; new namespaces
are additive.

---

## 6. WebSocket protocol additions

Current frames: client `{prompt|cancel}`; server
`{ready|message|thought|tool|done|error}`. Add (all optional, versioned via a
`ready` capability list so old apps keep working):

- client → `{type:"tool_result", id, value}` (app-defined tool returns),
  `{type:"approve"|"reject", id}`, `{type:"compact"}`, `{type:"snapshot"}`.
- server → `{type:"tool_call", id, name, args}` (when the agent calls an
  **app-defined** tool), `{type:"guardrail", name, blocked, reason}`,
  `{type:"approval", id, tool, args}`, `{type:"compaction", phase}`,
  `{type:"trace", span}`, `{type:"output", schema, value}` (structured output).

This is what lets an app **define its own tools** (`defineTool`) — the agent
calls back into the browser, the app executes JS and returns a result — closing
the "apps can't define tools" gap without a server round-trip.

**Open decision:** allow app-defined (browser-executed) tools? Powerful for
UI-driven tools, but the browser becomes a tool provider — needs the same
guardrail/approval treatment.

---

## 7. Phased roadmap

Each phase is independently shippable, compiles green, and has tests. Ordered by
value-per-risk.

**Phase 1 — Durable sessions + context API (recovery foundation).**
Bind app sessions to a stable id, persist/resume via `session_manager`; surface
`br.context.tokens/compact` + `compaction` events. Delivers "recover what it
lost" immediately. *Low risk, high value.*

**Phase 2 — Structured outputs + guardrail declarations.**
Add `output_type` JSON-Schema validation to the loop; compile manifest
`guardrails` (incl. `guardrails.goal`) onto the existing hook/inspector/goal
machinery; tripwire frames. Makes apps reliable. *Medium risk.*

**Phase 3 — Files + Data capabilities.**
Workspace-jailed `br.files`; `br.data` over existing MCP sources + a bundled
DuckDB SQL MCP. Capability-gated, deny-by-default. *Medium risk.*

**Phase 4 — Sandboxed compute.**
`biorouter::sandbox` with `LocalProcessSandbox` (label unsafe) + `DockerSandbox`
(default isolation); route `computercontroller`/`developer` tool calls through
it when `capabilities.compute` is set. *Highest risk — the security keystone.*

**Phase 5 — RunState + approvals (headless harness).**
Serializable mid-run state in the session DB; `br.approve/reject`; crash/
disconnect resume. Enables exported/headless apps to be safely autonomous.
*Medium risk; depends on Phase 1.*

**Phase 6 — App-defined tools + workflows + memory.**
`defineTool` (browser callbacks), manifest `workflow` steps via the generalized
sub-agent loop, KB-backed app memory with two-phase distillation. *Medium.*

**Phase 7 — Trace timeline + encrypted vault + GUI editors.**
Span tree → GUI timeline; per-app encrypted vault; capability/guardrail editors
in the Applications panel. *Lower risk, polish + governance.*

---

## 8. Security review (must-haves before any of this ships to multi-user)

1. **Compute isolation is non-negotiable for untrusted apps.** Until Phase 4,
   document loudly that generated apps run with full user privileges; gate
   `compute`/`shell` capabilities behind explicit opt-in + a warning.
2. **Workspace jail** must be enforced in Rust (canonicalize + prefix check),
   not just `.biorouterignore` (which is advisory and read-only-focused).
3. **Secrets by reference only** (§3.4) — never serialize plaintext secrets into
   manifests, app bundles, exports, or model context.
4. **Capability deny-by-default** — an app gets nothing it didn't declare *and*
   the user didn't authorize.
5. **App-defined tools and remote MCP** are exfiltration vectors (OpenAI's own
   docs warn "a malicious server can exfiltrate sensitive data") — apply the
   inspector chain + approval to them.
6. **Audit log** for tool calls / data queries / file writes (currently absent).
7. **Export trust boundary**: an exported app + `run.sh` self-installs and starts
   biorouterd — review that it can't be trojaned to point at a malicious endpoint
   or smuggle capabilities the user didn't grant.

---

## 9. What to borrow vs. what to keep distinct

**Borrow from OpenAI:** the control/compute split (`SandboxClient`), the
`Manifest` workspace concept, declarative input/output/**tool** guardrails with
tripwires, `RunState` serialize/resume, structured `output_type` as a contract,
the engine/provider/extensions package discipline, and the progressive-disclosure
memory idea.

**Keep distinctly BioRouter:** provider-agnosticism as the headline; the
`/goal` LLM-judge completion harness (genuinely ahead of OpenAI here); the
knowledge base as a first-class versioned memory/data substrate; scientific
visualization; and "apps as exportable, runnable products."

---

## 10. Decisions summary (the maintainer questions posed on 2026-06-23)

> **Note.** These questions were open when the RFC was written and this document records
> no answers to them. They were settled by what was subsequently built — see
> [the Apps SDK v2 design spec](../../apps-sdk/v2-design.md) and
> [the Apps SDK reference](../../apps-sdk/sdk-reference.md) for the design that shipped.

1. **Manifest shape** (§2.1) — single extended `manifest.json` as proposed?
2. **Deny-by-default capabilities** (§2.1) — adopt the OpenAI-style explicit-grant
   model over today's inherit-everything?
3. **Sandbox backend** (§3.5) — Docker as the default isolation layer (vs native
   OS sandboxing or staying in-process)?
4. **Structured outputs** (§3.3) — adopt JSON-Schema `output_type` as a contract?
5. **App memory substrate** (§3.7) — reuse the knowledge base?
6. **App-defined (browser-executed) tools** (§6) — allow them?
7. **Roadmap priority** (§7) — agree with the Phase 1→7 ordering, or re-rank
   (e.g., pull sandbox isolation earlier if multi-user is imminent)?
8. **Scope** — is the target single-user desktop (lighter security bar) or
   shared/multi-user deployment (Phase 4 + audit become blocking)?

---

## 11. Features beyond the original seven asks — inventory and recommendations

The original seven asks (files, databases, workflow orchestration, encrypt/sandbox
files, sandboxed compute, guardrailed/stop-hooked harness, context
recovery/compaction/sharing) map to the SDK's *infrastructure* core. An
exhaustive sweep of the actual `openai-agents-js` / `openai-agents-python` repos,
the developer docs, and the AgentKit product tier turned up **whole feature
categories that were not named.** Below: what each is, where BioRouter stands, and a
blunt **Adopt / Consider / Skip** call with rationale.

> **Note.** The verdicts in this section are the recommendations made on 2026-06-23. They
> are a historical record of what was proposed, not live guidance — several were adopted,
> and the shipped scope is described in [the Apps SDK v2 design spec](../../apps-sdk/v2-design.md).

> Legend — **ADOPT**: high value-per-effort, recommend building. **CONSIDER**:
> valuable but a real investment or a strategic bet. **SKIP/DEFER**: low ROI for
> BioRouter or actively being retired by OpenAI.

### Priority table

| # | Feature (beyond the original asks) | Rec | One-line why |
|---|---|---|---|
| 11.1 | Reliability & control primitives | **ADOPT** | Cheap, makes apps not break; some already half-built |
| 11.2 | Structured outputs as contracts | **ADOPT** | Turns `chart`-block guessing into typed guarantees |
| 11.3 | Safety guardrails: **PII/PHI, prompt-injection, groundedness** | **ADOPT ★** | The biggest miss — mandatory for biomedical/clinical data |
| 11.4 | Handoffs + agents-as-tools + deferred tools | **ADOPT** | Real multi-agent orchestration; the loop already exists |
| 11.5 | Human-in-the-loop approvals + serializable resume | **ADOPT** | Already in the plan (§3.6); reinforced here |
| 11.6 | Spanned tracing + GUI timeline (+ optional exporters) | **ADOPT** | Observability; the hook events are already emitted |
| 11.7 | Per-app model-routing surface + ModelSettings + usage | **ADOPT** | Doubles down on BioRouter's provider-agnostic headline |
| 11.8 | Lifecycle hooks exposed to apps | **CONSIDER** | App-level telemetry/control; they exist internally |
| 11.9 | Interactive widgets (forms/tables that call back) | **CONSIDER** | Extends charts to real UI round-trips |
| 11.10 | Voice / realtime agents (speech-to-speech, STT→agent→TTS) | **CONSIDER** | Compelling for hands-free lab/clinic; big build |
| 11.11 | Governed connector registry (RBAC + audit + creds) | **CONSIDER** | Becomes blocking for multi-user/clinical deployment |
| 11.12 | Durable execution (Temporal-style) | **DEFER** | Crash-proof long workflows; RunState+scheduler covers most |
| 11.13 | Visual workflow canvas (Agent-Builder-style) | **DEFER** | Nice GUI authoring — but OpenAI *retired theirs* |
| 11.14 | ChatKit embeddable UI / Apps-in-ChatGPT | **SKIP** | BioRouter already owns the app UI + export; niche |
| 11.15 | Hosted Evals platform + Reinforcement Fine-Tuning | **SKIP** | OpenAI is *shutting both down* (see §12) |

### 11.1 Reliability and execution-control primitives — **ADOPT**

A cluster of small knobs that separately look minor but together are why an
SDK-built agent "just works." None of these were in the original asks; most are cheap:

- **Error-to-final-output handlers** (`errorHandlers`/`error_handlers`, keys
  `maxTurns`/`modelRefusal`/`default`) — convert a blown turn-limit or a model
  refusal into a graceful answer instead of an exception. BioRouter today returns
  a raw "I've reached my action limit" string; this makes it a clean,
  app-renderable outcome.
- **Tool timeouts** (`timeoutMs` + `timeoutBehavior: error_as_result|raise`) and
  **`tool_not_found_behavior`** (`return_error_to_model` so the model
  self-corrects). BioRouter has retry (`agents/retry.rs`) but no per-tool timeout
  or graceful not-found.
- **Stop conditions** (`tool_use_behavior`: `stop_on_first_tool`, `StopAtTools`,
  `ToolsToFinalOutputFunction`) — declaratively end a turn on a chosen tool's
  output. Useful for "the last tool *is* the answer" app patterns.
- **The loop guard** (`reset_tool_choice`, default on) — after a forced tool
  call, reset `tool_choice` to auto so the model can't be trapped forcing tools
  forever. BioRouter's `max_turns` is the only backstop today; this is a cheaper,
  earlier guard.
- **Tool concurrency** (`max_function_tool_concurrency`) — run a turn's
  independent tool calls in parallel. BioRouter's sub-agent loop bundles results
  but executes serially.
- **Dynamic instructions** (a callback that builds the system prompt per turn),
  **`agent.clone(overrides)`**, and **`is_enabled` on tools & handoffs**
  (conditionally hide tools from the model). All let one app present different
  capabilities by state — e.g. hide the "write" tools until the user authenticates.

**Recommendation:** bundle these into Phase 2 (they ride along with the loop /
guardrail work). High reliability ROI, low surface area.

### 11.2 Structured outputs as contracts — **ADOPT**

`outputType`/`output_type` (Zod/Pydantic → JSON Schema, `strict:true`) makes the
final answer a *typed object*, not prose. Already proposed in §3.3 — restated
here because it's squarely "a feature that was not asked for." For BioRouter it
upgrades the heuristic `chart` code block into a validated contract and lets apps
declare "the agent must return `{gene, pathways[], citations[]}`."

### 11.3 Safety guardrails: PII/PHI, prompt-injection, groundedness — **ADOPT ★ (the standout)**

The original ask said "guardrails" generically (the harness/stop-hook ask, §3.6). But OpenAI
ships a **separate open-source guardrails package** (`openai-guardrails-python`/
`-js`, MIT) with *specific content checks* that BioRouter has **no equivalent
for**, and that matter enormously for a biomedical tool:

- **PII / PHI detection** (Microsoft Presidio, runs **locally**, mask-or-block) —
  for a UCSF tool touching clinical/OMOP/CDW data this is close to a compliance
  requirement, not a nicety.
- **Prompt-injection / "alignment" check** — validates that tool calls/outputs
  match user intent (defends against a malicious KB doc or web page hijacking the
  agent). Directly relevant since BioRouter apps can browse and read untrusted
  content.
- **Hallucination / groundedness** — checks answers against a vector store /
  cited sources. For scientific claims, "is this grounded in the KB?" is exactly
  the guardrail labs want.
- **Moderation, NSFW, off-topic (business-scope), URL allow/block, jailbreak,
  custom LLM-judge** — the rest of the pipeline.

BioRouter's `SecurityInspector` only matches *malware command patterns*; it does
nothing for PHI, injection, or grounding. This is the **single most valuable
thing the research surfaced that was not mentioned in the original asks.**

**Recommendation:** add a **content-guardrail pipeline** (pre-flight → input →
output stages, tripwires, config-driven like the manifest `guardrails` block in
§2.1) with PII/PHI + prompt-injection + groundedness as first-class checks. Run
PII locally; the LLM-judge checks use whatever provider the app already uses
(stays provider-agnostic). Slot into Phase 2 alongside the harness work, and make
PII/PHI **on-by-default** for any app whose `data` capability names a clinical
source.

### 11.4 Handoffs, agents-as-tools, deferred tools — **ADOPT**

OpenAI's orchestration is *two patterns on the `Agent` object*: **handoffs**
(`transfer_to_*`, transfer ownership) and **agents-as-tools** (`asTool()`,
manager keeps ownership), plus **deferred/lazy tools** (`tool_search` +
`defer_loading` to keep huge tool sets out of context until needed). §3.3 already
proposes generalizing BioRouter's sub-agent loop for agents-as-tools; add
handoffs (swap the active manifest agent) and lazy tool loading (matters once an
app wires many extensions). The `RECOMMENDED_PROMPT_PREFIX` + handoff input
filters (`remove_all_tools`, history mappers) are useful polish.

### 11.5 Human-in-the-loop + serializable `RunState` — **ADOPT** (already §3.6)

Restated as a "not-asked" feature: `needsApproval` → `interruptions` →
`state.approve()/reject()` → resume `run(agent, state)`, where `state`
**serializes across processes** (the JS/Python repos both show writing it to
disk/JSON and resuming, even in a different process). This is the foundation for
*headless* approvals and crash recovery (§3.6 Phase 5).

### 11.6 Observability: spanned tracing + exporters — **ADOPT** (extends §4)

Beyond the basic trace tree (§4): OpenAI exposes **pluggable processors/exporters**
and documents **26 third-party integrations** (Langfuse, Arize-Phoenix, MLflow,
Braintrust, Logfire, W&B, Datadog, LangSmith, Comet, PostHog, …). For BioRouter:
build the in-GUI span timeline first (provider-agnostic, no external dep), then
optionally expose a `TraceProcessor` hook so a lab that already runs Langfuse/
Phoenix can point BioRouter at it. Sensitive-data gating
(`trace_include_sensitive_data`) is a must given clinical content.

### 11.7 Per-app model-routing surface + ModelSettings + usage — **ADOPT**

This *amplifies BioRouter's headline advantage.* OpenAI exposes a rich
per-run model surface: `MultiProvider` prefix routing (`openai/…`, `litellm/…`,
`any-llm/…`), a deep `ModelSettings` (temperature, top_p, `reasoning` effort +
summary, `verbosity`, `max_tokens`, `parallel_tool_calls`, `truncation`,
`prompt_cache_retention`), reasoning-item replay, and a `Usage` object
(input/output/cached/reasoning tokens) on every result. BioRouter already routes
across 43+ providers internally — the gap is letting an **app** expose those
choices (a "model & settings" panel) and surface usage/cost back to the user.
Add `br.model.*` (list providers/models the *user* configured, pick, set
temperature/reasoning) + `br.context.usage()`.

### 11.8 Lifecycle hooks exposed to apps — **CONSIDER**

`RunHooks`/`AgentHooks` (`on_tool_start/end`, `on_handoff`, `on_llm_start/end`,
agent start/end). BioRouter *has* an internal hook system (`hooks/`) richer than
OpenAI's; the move is to **surface selected events to the app** (`br.on('tool',
'handoff', 'llm', 'compaction', …)`) so apps can render live telemetry, progress
bars, or audit trails. Cheap once the WS frame taxonomy (§6) is in.

### 11.9 Interactive widgets — **CONSIDER**

OpenAI's **Widgets** are a JSON component tree (Card/Row/Col/Table/Chart/Form/
Input/Select/Button…) streamed from the agent and rendered natively, with
**action round-trips** (`onSubmitAction`→server, `onClickAction`→client). Jinja
`.widget` templates author them. BioRouter already renders markdown + SVG charts
+ autovisualiser `ui://` figures; the new capability is **interactive output that
calls back into the agent** (a form the agent emits, the user fills, the result
re-enters the loop). This is essentially §6's app-defined-tools + a richer
renderer. Good fit, medium effort; pairs naturally with structured outputs
(11.2) as the contract for what the widget collects.

### 11.10 Voice / realtime agents — **CONSIDER (strategic)**

Two subsystems that neither the original asks nor the first plan mentioned: **realtime speech-to-
speech** (`RealtimeAgent`/`RealtimeSession` over WebRTC/WebSocket, barge-in/turn
detection, tools + handoffs + guardrails *in* the audio loop) and the **Python
voice pipeline** (STT→agent→TTS). For a lab/clinic, hands-free dictation and
voice querying of a knowledge base or OMOP cohort is genuinely compelling. It's a
big build (audio transport, turn detection, a realtime provider) and provider
coupling is tighter (realtime models are fewer), so it's a **strategic bet, not a
quick win** — but worth naming as a future Agent-Drafter app *modality*
(`createApp({ mode: 'voice' })`) rather than a one-off.

### 11.11 Governed connector registry — **CONSIDER (blocking for multi-user)**

OpenAI's **Connector Registry** is a default-deny, admin-governed catalog of
data/tool connections (RBAC, encrypted credentials w/ OAuth refresh, audit logs)
keyed on stable `connector_id`s over MCP. BioRouter has the *catalog* half (BAAM
`registry.json` + extension manager) but **no credential governance, RBAC, or
audit**. For single-user desktop this is optional; for any shared/clinical
deployment it becomes blocking (ties to §8's audit-log gap). Borrow the pattern:
extend BAAM entries with auth scope + read-only + audit hooks, resolve creds from
the keyring by reference (§3.4).

### 11.12 Durable execution — **DEFER**

OpenAI documents a **Temporal** integration (maintained by Temporal, Python-only)
that wraps the agent loop as durable workflow/activity steps → automatic retries,
state persistence, crash recovery for long/async/human-in-the-loop runs.
BioRouter's `RunState` (§3.7) + `scheduler.rs` + session persistence cover most
of the "survive a crash" need without a workflow engine. Defer unless apps start
running multi-hour pipelines.

### 11.13 Visual workflow canvas — **DEFER (note: OpenAI retired theirs)**

**Agent Builder** is a drag-and-drop node graph (agent/tool/if-else/while/
human-approval/guardrail/transform/state nodes, CEL expressions, versioned
publish, **export-to-SDK-code**). A GUI canvas atop BioRouter's manifest
workflows (§3.3) would be a strong authoring story — but note OpenAI
**deprecated Agent Builder (shutdown Nov 30 2026)** without it ever reaching GA,
steering users back to *code-first* SDK workflows. Treat the canvas as a
validated-but-retired blueprint: a later GUI nicety, not a foundation.

### 11.14 ChatKit / Apps-in-ChatGPT — **SKIP**

ChatKit (embeddable themeable chat UI with a two-leg session-token model) and
Apps-in-ChatGPT (apps that run *inside* ChatGPT over MCP) solve "embed an agent
UI elsewhere" / "distribute inside ChatGPT." BioRouter already **owns** its app
UI, serves apps, and exports standalone runnable folders — it's its own hub. Skip
unless a concrete "embed a BioRouter app in an external site" requirement appears.

### 11.15 Hosted Evals platform + Reinforcement Fine-Tuning — **SKIP**

OpenAI's **Evals for Agents** (datasets, trace grading, automated prompt
optimization) and **RFT/fine-tuning** are being **wound down** (Evals platform
read-only Oct 31 2026, shutdown Nov 30 2026; fine-tuning/RFT closed to new jobs
by Jan 2027; recommended eval migration is Promptfoo). Don't build a heavy evals
platform. The *cheap, durable* idea worth borrowing is **trace grading** — let
the GUI replay a run's span tree (11.6) and attach a thumbs-up/down + an
LLM-judge score, feeding a lightweight regression set. That's a feature on top of
tracing, not a platform.

---

## 12. Strategic note: OpenAI is retiring its hosted product tier

A clear signal from the deprecations page (primary source): OpenAI is shutting
down **Agent Builder** (the visual canvas), the **Evals platform**, and
**fine-tuning/RFT** within ~12–18 months, while keeping the **open-source Agents
SDK** (incl. tracing), **ChatKit** (frontend), and **Guardrails** alive. The
durable bet is the **code-first SDK primitives**, not the hosted products.

**What this means for BioRouter:**

- BioRouter already *is* the durable, code-first, provider-agnostic layer OpenAI
  is steering people back toward. Lean into that — it's the right side of this
  trend.
- Prioritize borrowing the **library-level** primitives (guardrails, structured
  outputs, HITL/RunState, tracing, model settings, reliability knobs) over the
  **product-level** ones (visual builder, hosted evals).
- Be cautious sinking effort into a visual workflow canvas or a hosted evals
  platform — OpenAI just demonstrated those are the first things to get cut.

---

## 13. Updated decisions (extends §10)

These continue the numbering of §10 and, like it, were left unanswered in this document.

9. **Content guardrails (11.3)** — adopt a PII/PHI + prompt-injection +
   groundedness pipeline, PII/PHI on-by-default for clinical-data apps? *(Strong
   recommend — the standout gap.)*
10. **Reliability cluster (11.1)** — fold error-to-output handlers, tool
    timeouts, stop conditions, the loop guard, and tool concurrency into Phase 2?
11. **Model surface for apps (11.7)** — expose per-app model/provider/settings +
    usage to amplify the provider-agnostic advantage?
12. **Voice/realtime (11.10)** — is a voice app modality on the roadmap, or out
    of scope for now?
13. **Interactive widgets (11.9)** — build agent-emitted interactive widgets
    (forms/tables with action round-trips), or keep output read-only
    (markdown/charts)?
14. **Governance (11.11)** — build the connector-registry-style RBAC/audit/cred
    governance now (only if multi-user/clinical deployment is in scope)?
15. **Skip confirmations** — agree to skip ChatKit-embedding, Apps-in-ChatGPT,
    and a hosted evals/RFT platform (per §12)?

### Updated roadmap deltas

- **Phase 2** absorbs: structured outputs (11.2), reliability cluster (11.1),
  **and the content-guardrail pipeline (11.3)** — elevate guardrails to a Phase-2
  headline, not just the goal-hook.
- **Phase 6** absorbs: handoffs + deferred tools (11.4) and interactive widgets
  (11.9) alongside app-defined tools.
- **New optional tracks** (unscheduled, pick if relevant): per-app model surface
  (11.7, small — pull early), lifecycle-hook exposure (11.8, rides on §6),
  voice modality (11.10, strategic), connector governance (11.11, gated on
  multi-user), durable execution (11.12) and visual canvas (11.13) deferred.

---

## Appendix A — key BioRouter files referenced

- Agent loop / turns: `crates/biorouter/src/agents/agent.rs`
- Hooks: `crates/biorouter/src/hooks/` (`mod.rs`, `event.rs`)
- Goal harness: `crates/biorouter/src/agents/goal.rs`; retry: `agents/retry.rs`
- Sub-agent loop: `crates/biorouter-mcp/src/knowledge/subagent/loop_.rs`
- Context mgmt / compaction: `crates/biorouter/src/context_mgmt/mod.rs`;
  token counting: `crates/biorouter/src/token_counter.rs`
- Sessions: `crates/biorouter/src/session/session_manager.rs`
- Security / permissions: `crates/biorouter/src/security/`,
  `crates/biorouter/src/permission/`, `crates/biorouter/src/tool_inspection.rs`
- Secrets / keyring: `crates/biorouter/src/config/base.rs`
- Developer tools: `crates/biorouter-mcp/src/developer/`
- Computer Controller: `crates/biorouter-mcp/src/computercontroller/`
- Knowledge store: `crates/biorouter-mcp/src/knowledge/`
- Agent Drafter: `crates/biorouter-mcp/src/agent_drafter/` (`mod.rs`, `store.rs`,
  `bundle.rs`, `render.rs`, `templates/sdk.ts`)
- App Runner: `crates/biorouter-server/src/routes/apps.rs`

## Appendix B — OpenAI sources (grounded)

- `github.com/openai/openai-agents-js` (README, docs guides, `examples/docs/
  sandbox-agents/basic.ts`, `human-in-the-loop/index.ts`, package.json's)
- `github.com/openai/openai-agents-python` (`src/agents/__init__.py`,
  `src/agents/sandbox/**`, docs guides)
- `developers.openai.com/api/docs/guides/agents` (+ orchestration, guardrails-
  approvals, sandboxes, tools-file-search, tools-code-interpreter, structured-
  outputs)
- Announcement-level (AgentKit / Agent Builder / ChatKit / Connector Registry /
  Evals): secondary coverage — **verify in a browser before quoting**.

## Related documentation

- [App SDK implementation design RFC](implementation-design.md) — the code-level companion written one day later, which turns this inventory into Rust types, WebSocket frames and hook points.
- [Apps SDK v2 design](../../apps-sdk/v2-design.md) — the 2026-07-12 spec that superseded this RFC and answered its open decisions.
- [Apps SDK reference](../../apps-sdk/sdk-reference.md) — what the `br.*` surface and manifest actually look like as shipped.
- [Apps SDK v2 phase roadmap](../../apps-sdk/v2-phase-roadmap.md) — the phase plan that replaced §7's roadmap.
- [Agent Drafter apps platform design](../../agent-drafter/apps-platform-design.md) — the app platform this SDK was designed to extend.
