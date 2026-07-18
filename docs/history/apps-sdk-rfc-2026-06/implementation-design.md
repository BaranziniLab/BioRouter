# BioRouter App SDK (BRSDK) — Implementation-Level Design

> Status: **implementation design / RFC**. Authored 2026-06-24. Companion to
> [docs/agent-drafter-sdk-plan.md](agent-drafter-sdk-plan.md) (the strategy +
> feature inventory). This doc is the code-level "how": concrete Rust
> types/traits/signatures, the TypeScript client surface, the WebSocket protocol
> v2, manifest schema, the exact hook points in the existing code (with
> `file:line`), new files/crates, schema migrations, a phased build order, and a
> consolidated test plan.
>
> Scope = everything in the original seven asks **plus** the "✅ worth adding"
> set (content guardrails incl. PII/PHI, structured outputs, the reliability
> cluster, handoffs/agents-as-tools/lazy tools, HITL + serializable RunState,
> spanned tracing + GUI timeline, per-app model surface) **plus** interactive
> widgets (agent-emitted forms/tables that call back into the loop).
>
> It was produced by reading the real tree. Where an agent's assumption was
> corrected by the code, the correction is folded in (e.g. schema version is
> **8**, the resume primitive is `get_session`, not `load_session`).

---

## 0. How to read this

- **§1 Shared backbone** is authoritative and global: the unified manifest, the
  full WS v2 frame set, the `br` client shape, the integration map, and the
  code-level corrections every cluster depends on. Read it first.
- **§2–§6** are the five capability clusters at implementation level.
- **§7** resolves the cross-cluster conflicts the design surfaced.
- **§8** is the dependency-ordered build order; **§9** the test plan; **§10**
  the consolidated open questions/decisions.

Everything is **additive and back-compatible**: existing Agent-Drafter apps
(manifest = old `AgentConfig`, client = `br.run/prompt/ask/on`, bundled
`dist/app.js`) keep working untouched. New behavior is gated behind manifest
declarations (deny-by-default) and advertised in the `ready` frame's capability
list so old apps never see frames they don't understand.

---

## 1. Shared backbone

### 1.1 Code-level corrections (verified against the tree — these shape everything)

1. **Session schema version is `8`** (`session/session_manager.rs:21`
   `CURRENT_SCHEMA_VERSION: i32 = 8`), not 7. New columns/tables = **migration
   v9** (added to both `apply_migration` and `create_schema`).
2. **The resume primitive is `SessionManager::get_session(id, true)`**
   (`session_manager.rs:304`) — there is no `load_session`. The agent's
   `reply()` reads the conversation from the store itself
   (`agents/agent.rs:1121`), so **reusing a stable session id makes the agent
   resume automatically** — no conversation copying.
3. **Built-in MCP servers are constructed with no per-app context.**
   `BUILTIN_EXTENSIONS` spawns each via `(def.spawn_server)(r,w)` → `::new()`
   (`biorouter-mcp/src/lib.rs:53-66`); the working dir is process-global
   (`std::env::current_dir()` / `BIOROUTER_WORKING_DIR`). So the workspace jail
   and the sandbox cannot be injected through the registry — the app runner must
   construct **per-app** `DeveloperServer::with_jail(...)` /
   `ComputerControllerServer::with_jail(...)` and feed them into the per-app
   extension manager (new `ExtensionConfig::JailedBuiltin` variant, or an
   out-of-band server injection in `handle_agent_socket`).
4. **Hooks are fire-and-forget** (shell commands / LLM judges) and the WS route
   only ever sees the `AgentEvent` stream from `agent.reply()`
   (`agent.rs:164-169`). Compaction/sub-agent/session hook events do **not**
   reach the route. ⇒ tracing + lifecycle exposure require a **new in-process
   observer bus** (`tokio::sync::broadcast`, §5) published at the existing hook
   firing sites; observe-only, never blocks the loop.
5. **There is no `tool_choice` plumbing** in the streaming path
   (`agents/reply_parts.rs:173` `stream_response_from_provider` passes none).
   `reset_tool_choice` / `tool_use_behavior` are net-new plumbing (phased — see
   §4 open Q).
6. **`templates/sdk.ts` is outside the vitest scope** (`ui/desktop` only) and
   has **no existing test**. New SDK code must also survive the **fallback
   type-stripper** in `bundle.rs` (no `enum`/`namespace`/decorators/`satisfies`/
   `import type`; types only in `interface`/`type`; generics only in signatures,
   never value positions; es2018). Tests added via a vitest alias to the real
   file (§6).

### 1.2 The unified manifest (reconciles all clusters)

Two clusters independently proposed a `capabilities` field with different
shapes; one proposed `guardrails`; one proposed `reliability` + orchestration
fields. **Reconciled into one `AgentConfig`** in
`crates/biorouter-mcp/src/agent_drafter/store.rs` (extends the existing struct;
every new field `#[serde(default)]` so old manifests load unchanged).

```rust
pub struct AgentConfig {
    // ── existing ──
    pub system_prompt: String,
    pub greeting: Option<String>,
    pub tools: Vec<String>,
    pub model: Option<ModelSelection>,          // now carries `.settings`
    pub extensions: Vec<String>,
    pub skills: Vec<String>,
    pub knowledge_base: Option<String>,
    pub max_turns: Option<u32>,

    // ── NEW: one capabilities block (deny-by-default; absence = denied) ──
    #[serde(default)] pub capabilities: Capabilities,
    // ── NEW: declarative safety ──
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guardrails: Option<GuardrailsConfig>,
    // ── NEW: reliability knobs ──
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reliability: Option<ReliabilityConfig>,
    // ── NEW: multi-agent orchestration ──
    #[serde(default)] pub orchestration: Orchestration,
    // ── NEW: structured-output contract ──
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_type: Option<serde_json::Value>,
    // ── NEW: durable resumable sessions (default ON — the recovery fix) ──
    #[serde(default = "default_true")] pub durable_session: bool,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct Capabilities {
    pub files:   Option<FilesCapability>,    // §2.1
    pub data:    Option<DataCapability>,     // §2.2
    pub compute: Option<ComputeCapability>,  // §2.3
    pub vault:   Option<VaultCapability>,    // §2.4
    #[serde(default)] pub memory:  MemoryCapability,   // §5c (default Off)
    #[serde(default)] pub tracing: TracingCapability,  // §5d (default off)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,                 // §5e lifecycle events exposed to br.on()
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct Orchestration {
    #[serde(default)] pub sub_agents: HashMap<String, SubAgentManifest>, // agents-as-tools
    #[serde(default)] pub agents:     HashMap<String, AgentConfig>,      // handoff targets
    #[serde(default)] pub workflows:  HashMap<String, WorkflowManifest>, // declarative steps
    #[serde(default)] pub lazy_tools: bool,                             // defer tool schemas
}

// ModelSelection gains:
pub struct ModelSelection {
    pub provider: Option<String>,
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<ModelSettings>,     // §4d temperature/reasoning/etc.
}
```

Per-cluster sub-structs (`FilesCapability`, `DataCapability`, `ComputeCapability`,
`VaultCapability`, `MemoryCapability`, `TracingCapability`, `GuardrailsConfig`,
`ReliabilityConfig`, `ModelSettings`, `SubAgentManifest`, `WorkflowManifest`)
are defined in their cluster sections. The `create_app`/`configure_app` MCP tool
params (`agent_drafter/mod.rs`) gain matching optional fields; `bundle.rs::lint_app`
validates them (e.g. groundedness-without-KB → warn; `needs_approval` naming an
absent tool → warn).

### 1.3 WebSocket protocol v2 (full frame set)

Existing (v1) frames unchanged. The **`ready` frame advertises a capability
list + protocol version + resumed session id** so old apps ignore new frames and
new apps degrade gracefully against an old server. `ClientFrame`
(`routes/apps.rs:214`) and the server emitter gain the additive variants.

**Server → client**
| type | payload | source |
|---|---|---|
| `ready` | `{protocol:2, capabilities:[…], sessionId?, clientKey?, resumed?, messageCount?}` | §5a |
| `message`/`thought` | `{delta}` | v1 |
| `tool` | `{name, status, id?}` (now real name+id) | §5e |
| `done` / `error` | — / `{message}` | v1 |
| `output` | `{schema?, value}` (structured-output result; also the RPC envelope `{value:{__reqId,result|error}}`) | §4b/§6 |
| `usage` | `{requests,input_tokens,output_tokens,cached_tokens?,reasoning_tokens?,total_tokens,model}` | §4d |
| `guardrail` | `{stage,name,blocked,reason}` | §3a |
| `approval` | `{id,tool,args,reason?}` | §3c |
| `tool_call` | `{id,name,args}` (app-defined tool callback) | §4/§6 |
| `handoff` | `{from,to}` | §4a |
| `compaction` | `{phase,trigger?,before?,after?}` | §5b |
| `trace` | `{span}` or `{snapshot:[…]}` | §5d |
| `widget` | `{id,tree}` | §6b |
| `context` | `{used,limit,ratio}` | §5b |
| `data_result` / `compute_result` | per §2 | §2 |

**Client → server**
| type | payload | source |
|---|---|---|
| `prompt` | `{text, images?, output_type?}` | v1 + §4b |
| `cancel` | — | v1 |
| `approve` / `reject` | `{id}` / `{id, reason?}` | §3c |
| `tool_result` | `{id, value?` \| `error?}` | §4/§6 |
| `register_tools` | `{tools:[{name,description?,schema}]}` (on-connect, app-defined tools) | §6c |
| `widget_action` | `{widgetId, action, payload}` | §6b |
| `model_select` | `{provider?, model?, settings?}` | §4d |
| `workflow_run` | `{name, input}` | §4a |
| `handoff` | `{to}` | §4a |
| `compact` / `snapshot` / `restore` | — / — / `{id}` | §5b |
| `tokens` | — | §5b |
| `data_query` / `compute_run` | per §2 | §2 |

### 1.4 The `br` client object (namespaced, back-compatible)

`templates/sdk.ts` grows from one `BioRouterClient` class into the **same class
with namespace properties**. All existing members (`connect/on/off/prompt/ask/
run/cancel`, `renderMarkdown/renderChart/fileToImageInput/mountChat`, `createApp`
returning the instance and setting `window.BioRouter`) are retained verbatim. A
single **frame-dispatch table** routes every server frame to a typed event +
built-in side-effect; unknown frames are forwarded by name (forward-compat) and
never throw. New namespaces (capability-gated via `br.has(cap)`):

```
br.files.{list,read,write,upload,url,search}          §2.1
br.data.{sources,query,table}                         §2.2
br.compute.{run,python,cancel}                        §2.3
br.workflow.{run,onStep}                              §4a
br.context.{tokens,compact,snapshot,restore}          §5b
br.model.{list,select,settings}                       §4d
br.widgets.{render,get,action,mount}                  §6b
br.approve(id) / br.reject(id,reason?)                §3c
br.defineTool(name, schema, handler)                  §6c
br.on('guardrail'|'approval'|'output'|'usage'|'trace'|'compaction'|'handoff'|'tool_call'|'widget'|'llm'|'session', fn)
```

### 1.5 Integration map (where it all hooks in)

| Site (`file:line`) | What attaches |
|---|---|
| `routes/apps.rs:235` `configure_agent` | apply `model.settings`; install capabilities (jailed builtins, data sources, sandbox, vault ctx); compile `guardrails` → pipeline + goal + `needs_approval` permissions; register `orchestration.sub_agents`; set `output_type`/`reliability` on `SessionConfig`; resolve `kb_root` |
| `routes/apps.rs:347` `handle_agent_socket` | durable resume (client_id → external_key); v2 `ClientFrame` variants; map new `AgentEvent`s → frames; subscribe `ObsBus` → `trace`/lifecycle frames; `approve/reject`→`resume_run`; `model_select`→`update_provider`; `handoff` swap; widget tool-result→`widget` frame |
| `agent.rs:978` (post `user_prompt_submit`) | pre-turn guardrails (PII mask, moderation) |
| `agent.rs:284`/`:1404` inspector chain | `GuardrailInspector` (PII + injection on tool input) |
| `agent.rs:1487` tool-result drain | tool-output guardrails |
| `agent.rs:1811` (beside Stop hook) | output guardrails (groundedness) + goal handling |
| `agent.rs:1717-1778` natural-completion | structured-output validate + bounded re-prompt |
| `agent.rs:660`/`:1438`/`:668`/`:1712`/`:1277` | reliability: timeouts / concurrency / not-found / stop-conditions / error→output |
| `agent.rs:872` `list_tools`, `:617` `dispatch_tool_call` | agents-as-tools registration + dispatch; lazy-tool filter |
| `agent.rs:262`,`:1526`,`:986`,`:1798`, `subagent_handler.rs:140` | `ObsBus.publish(...)` next to each hook fire |
| `session_manager.rs:21`/`:709`/`:947` | schema v9: `external_key` column + `agent_run_states` table; `get_or_create_by_external_key`, `save/load_run_state` |
| `developer/rmcp_developer.rs:1545` `resolve_path`, `:1229` `execute_shell_command` | workspace jail; sandbox delegation |
| `config/base.rs:845` `set_secret` | vault data-key storage |

### 1.6 New crates / modules / files (master list)

**Engine (`crates/biorouter/src/`)**
- `sandbox/{mod.rs,local.rs,docker.rs}` — `SandboxClient` trait + impls (placement caveat §7.3).
- `guardrails/{mod.rs,pii.rs,checks.rs,run_state.rs}` — pipeline, local PII/PHI, LLM-judge checks, serializable RunState.
- `agents/subagent/{mod.rs,extension_dispatch.rs}`, `agents/agent_as_tool.rs`, `agents/lazy_tools.rs`, `agents/app_workflow.rs`, `agents/structured_output.rs`, `agents/reliability.rs`.
- `observability/{mod.rs,trace.rs,processors/…}` — `ObsBus`, span tree, optional Langfuse/Phoenix/OTLP adapters.

**MCP (`crates/biorouter-mcp/src/`)**
- `developer/jail.rs` — `Jail` (canonicalize + prefix + symlink-hardened).
- `datasql/mod.rs` — `DataSqlServer` (DuckDB/SQLite, read-only-by-default) registered in `BUILTIN_EXTENSIONS`.
- `agent_drafter/vault.rs` — DK gen/wrap via keyring, `.vault/` AES-GCM, `{{vault:NAME}}` resolver.

**Server (`crates/biorouter-server/src/`)**
- `apps/distill.rs` — two-phase KB lesson distillation job.
- routes/handlers in `apps.rs` (files HTTP, session backlog, models, v2 frames).

**Frontend (`ui/desktop/src/`)**
- `components/applications/{AppCapabilityBadges,ApprovalPrompt,TraceTimeline}.tsx`, `hooks/useAppAgentSocket.ts`, `sdk/__tests__/sdk.test.ts` (+ `vitest.config.ts` alias to the real `sdk.ts`).

**New deps**: `aes-gcm`, `rand`, `aho-corasick`, `duckdb` (or `rusqlite`), `hmac`,
`sha2`, `jsonschema`, axum `multipart` feature.

### 1.7 Schema migration v9 (one migration, two changes)

Both the durable-session column and the RunState table land in **v9** (bump
`CURRENT_SCHEMA_VERSION` 8→9; add to `apply_migration` *and* `create_schema`):

```sql
ALTER TABLE sessions ADD COLUMN external_key TEXT;
CREATE UNIQUE INDEX idx_sessions_external_key
  ON sessions(external_key) WHERE external_key IS NOT NULL;
CREATE TABLE agent_run_states (
  run_id     TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id),
  app_id     TEXT NOT NULL, status TEXT NOT NULL, state_json TEXT NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP);
CREATE INDEX idx_run_states_session ON agent_run_states(session_id);
```

---

## 2. Cluster 1 — Files · Databases · Sandboxed compute · Vault (data plane)

### 2.1 File access (workspace jail)

Manifest:
```rust
pub struct FilesCapability { pub entries: Vec<WorkspaceEntry>, pub max_file_bytes: Option<u64> }
pub struct WorkspaceEntry { pub name: String, pub local_dir: Option<String>, pub mode: String /*ro|rw*/, pub out_dir: bool }
```
Default root `~/.config/biorouter/agent_drafter/<id>/workspace/`; `entries` mount
extra host dirs ro/rw.

`crates/biorouter-mcp/src/developer/jail.rs` — `Jail { roots: Vec<(PathBuf,bool)> }`
with `resolve(requested, need_write) -> Result<PathBuf, ErrorData>`: reject any
`..` component, then **canonicalize the deepest existing ancestor** (covers
new-file paths) and verify the real path `starts_with` an allowed root (defeats
symlink escape); enforce ro/rw. `DeveloperServer` gains `Option<Jail>` and
`with_jail(jail, sandbox)`; the check replaces the CWD logic at the single choke
point `resolve_path` (`rmcp_developer.rs:1545`) — every file op already routes
through it. `.biorouterignore` runs *after* the jail (add `**/.vault/**`).

HTTP routes (bulk bytes off the WS), added to `routes()` (`apps.rs:480`):
`GET /apps/{id}/files` (list), `GET|PUT /apps/{id}/files/{*path}`,
`POST /apps/{id}/files/upload`, `POST /apps/{id}/files/sign`. Auth: extend
`check_token` (`auth.rs`) to allow `/apps/{id}/files/*` **with a valid signed
token** (HMAC-SHA256 over `id|path|method|exp`, keyed by daemon secret, ≤120s
TTL) regardless of method — `files_sign` is secret-key-gated and mints
jail-scoped tokens (injected into `BIOROUTER_APP_CONFIG` at serve time). Same
`jail_for(manifest)` used by both HTTP and the agent's editor. 403 when
`capabilities.files` is None.

Client: `br.files.{list,read,write,upload,url,search}` — `url(path)` returns the
signed GET URL for `<img src>`/downloads; `search` uses the workspace BM25.

### 2.2 Databases

```rust
pub struct DataCapability { pub sources: Vec<DataSource> }
pub struct DataSource { pub name:String, pub kind:String /*knowledge|spoke|omop|cdw|sql*/,
                        pub file:Option<String>, pub ref_id:Option<String>, pub read_only:bool }
```
New bundled `DataSqlServer` (`crates/biorouter-mcp/src/datasql/`, registered in
`BUILTIN_EXTENSIONS`), DuckDB preferred (native CSV/Parquet) / SQLite fallback,
opened read-only unless `read_only=false`; db file path resolved through the same
`Jail`. rmcp tools `data_sources`, `data_query{source,sql,max_rows?}`,
`data_table{source,table}`; non-SELECT rejected when read-only; result capped
(~5k rows/1MB, like `validate_shell_output_size`). Extension-backed sources
(knowledge/SPOKE/OMOP/CDW) are *enumerated* by `data_sources` but routed to their
own extension tools. Client `br.data.{sources,query,table}`.

### 2.3 Sandboxed compute

```rust
pub struct ComputeCapability { pub sandbox:String /*none|local|docker*/, pub timeout_s:u64,
                               pub network:String /*none|host*/, pub max_mem:Option<String>, pub image:Option<String> }
```
New `crates/biorouter/src/sandbox/`:
```rust
#[async_trait] pub trait SandboxClient: Send + Sync {
    fn label(&self) -> &'static str;
    async fn exec(&self, argv:&[String], stdin:Option<&str>) -> Result<ExecOutput>;
    async fn read/write/list(&self, …) -> Result<…>;
    async fn snapshot(&self) -> Result<String>;
}
pub fn create(spec: SandboxSpec, kind:&str) -> Result<Arc<dyn SandboxClient>>;
```
- `LocalProcessSandbox` — today's behavior, `label()="local(unsafe)"`, logs a
  warning on construction.
- `DockerSandbox` — `docker run --rm --network none -v <ws>:/work:rw -w /work
  --cpus N --memory <max_mem> --pids-limit 256 --cap-drop ALL --security-opt
  no-new-privileges --user 65534:65534 <image> <argv>`; `--network host` only if
  `network=host`. `snapshot()` = `docker commit` or workspace tar.

Wire at the real shell choke points: `DeveloperServer::execute_shell_command`
(`rmcp_developer.rs:1229`, the `command.spawn()` at `:1266`) and
`computercontroller`'s `automation_script_impl`/`computer_control_impl`
(`mod.rs:684`) delegate to `self.sandbox.exec(...)` when set, racing the existing
`cancellation_token`. `compute=none` (default) ⇒ no sandbox ⇒ those extensions
aren't granted. Client `br.compute.{run,python}`.

### 2.4 Encryption / vault

```rust
pub struct VaultCapability { pub encrypted: Vec<String> } // secret names referenceable
```
`crates/biorouter-mcp/src/agent_drafter/vault.rs`: per-app 32-byte data key (DK)
stored as a normal keyring secret via `Config::set_secret("agent_drafter.<id>.vault_dk", …)`
(`config/base.rs:845`, transparently OS-keyring or file-fallback). Secret values
live at `workspace/.vault/<NAME>.enc` = `nonce(12)||AES-256-GCM(value)` (crate
`aes-gcm`). `.biorouterignore` masks `.vault/`. **`{{vault:NAME}}` resolves
server-side at tool-call time, only for names in `capabilities.vault.encrypted`,
via a per-app `ToolInspector`/wrapper installed by `apps.rs`** (keeps the engine
capability-agnostic; plaintext never enters WS frames or model context). Export
(`render.rs:scaffold_standalone`) carries `.vault/*.enc` + a wrapped-DK sidecar,
re-bound to the new machine's keyring on first run (then sidecar deleted).

**Test plan**: jail unit (`..`, absolute-outside, symlink escape, ro/rw,
multi-root); jail-escape integration through `text_editor`; HTTP signed-token
(unsigned 401, expired 401, cross-app 403, traversal 403, no-capability 403);
`SandboxClient` (local echo+timeout; Docker arg-builder asserts
`--network none/--cap-drop ALL/--user`, run gated on `which docker`); DataSql
(read-only blocks DML, row cap, db outside jail rejected); vault (encrypt/decrypt
roundtrip, `{{vault}}` resolved only for allow-listed names, frames never carry
plaintext, export sidecar re-bind).

---

## 3. Cluster 2 — Content guardrails · Goal harness · HITL + RunState

### 3.1 Guardrail pipeline

```rust
pub enum GuardrailStage { PreFlight, Input, ToolInput, ToolOutput, Output }
pub enum GuardrailVerdict { Pass, Mask{masked_text:String, finding:String}, Trip{reason:String, blocked:bool} }
#[async_trait] pub trait Guardrail: Send+Sync {
    fn name(&self)->&'static str; fn stages(&self)->&'static [GuardrailStage];
    async fn check(&self, ctx:&GuardrailContext<'_>) -> Result<GuardrailVerdict>;
}
```
`GuardrailPipeline` runs stage-matching guards (fail-open on `Err`, matching the
existing `inspect_tools` doctrine). Stage wiring (cluster A found the exact
sites):
- **ToolInput** → a new `GuardrailInspector` in the existing `ToolInspector`
  chain (`agent.rs:284`/`:1404`): `Trip{blocked}`→`Deny`, `Trip{!blocked}`→
  `RequireApproval`, `Mask`→rewrite args + `Allow`. Inherits "most-restrictive
  wins" for free.
- **PreFlight/Input** → new pre-turn stage after `user_prompt_submit`
  (`agent.rs:978`).
- **ToolOutput** → tool-result drain (`agent.rs:1487`).
- **Output** → post-turn beside the Stop hook (`agent.rs:1811`) — runs on the
  complete answer (note: output blocking is a post-stream retraction/warning, not
  mid-stream suppression — open Q).

Checks (`guardrails/checks.rs` + `pii.rs`):
- **`PiiGuard`** (LOCAL, `pii.rs`): regex + Luhn + keyword-anchored detectors for
  SSN/MRN/DOB/phone/email/address/credit-card + an `aho-corasick` name
  dictionary; `scan()`/`mask()`. `scan()` is the single seam where a Presidio/ONNX
  NER can later plug in. Modes off/mask/block.
- **`InjectionAlignmentGuard`**, **`GroundednessGuard`** (BM25 over the active KB
  via `knowledge::store::search`, `store.rs:172`), **`ModerationScopeGuard`** —
  all LLM-judge, reusing the proven `prompt_runner::run_prompt_hook` contract
  (`hooks/prompt_runner.rs:45`: `provider.complete_fast(...)` + `parse_verdict`).
  **Provider-agnostic by construction** — they use the app's configured provider
  (`agent.provider().await`), fail-open when none.

Manifest:
```rust
pub struct GuardrailsConfig { pub goal:Option<String>, pub business_scope:Option<String>,
  pub pii:PiiModeCfg /*off|mask|block*/, pub checks:Vec<GuardrailCheckCfg>,
  pub needs_approval:Vec<String>, pub approvals_require_persistence:bool }
pub struct GuardrailCheckCfg { pub kind:String /*injection|groundedness|moderation*/,
  pub stages:Vec<String>, pub on_trip:String /*block|fail|warn*/ }
```
Tripwire → always emit `guardrail{}` frame; then Block/Fail/Warn per config.

### 3.2 Goal/Stop-hook harness (one-liner)

`guardrails.goal: String` → in `configure_agent`, call `agent.set_goal(session_id, goal)`
(`agents/goal.rs:268`), which installs the session-scoped Stop-hook judge with the
existing iteration-cap (`GOAL_MAX_ITERATIONS=20`), stall detection, and graceful
give-up. One field, zero new engine code. Auto-`clear_goal` on socket close.

### 3.3 HITL approvals + serializable RunState

Per-tool `needs_approval` → `update_permission_manager(tool, AskBefore)` so the
existing `PermissionInspector` yields `RequireApproval` (`agent.rs:1448`).
Interactive path reuses `ActionRequiredManager` (`action_required_manager.rs:39`)
+ new `approval{}` frame ↔ `br.approve/reject`. Non-interactive path
(`approvals_require_persistence` or a client-advertised non-interactive flag):
snapshot to a serializable `RunState` (`guardrails/run_state.rs`):
```rust
pub struct RunState { pub version:u32, pub run_id:String, pub session_id:String, pub app_id:String,
  pub status:RunStatus, pub pending_tool:PendingTool, pub reason:String,
  pub conversation:Conversation, pub session_config:SessionConfig,
  pub remaining_turns:u32, pub goal:Option<GoalState> }
```
(`Conversation`/`SessionConfig` already derive serde; add derives to `GoalState`.)
Persist in the v9 `agent_run_states` table (`save/load/update/list_pending_run_states`).
`Agent::resume_run(run_id, approved, reason)`: on approve → dispatch the pending
tool, splice result, continue with `remaining_turns`, re-install goal; on reject →
inject the `DECLINED_RESPONSE` tool response so the model adapts. WS
`approve/reject` frames call it and re-stream events.

Two new `AgentEvent` variants (`agent.rs:163`): `Guardrail{…}`,
`ApprovalRequired{run_id,tool,args,reason}`; mapped to frames in the WS relay
(`apps.rs:428`).

**Test plan**: PII per-type + false-positive rejection + mask roundtrip;
pipeline stage-matching + fail-open + trip mapping; `GuardrailInspector`
Deny/Allow/mask; judge guards with a stub provider (trip / no-provider fail-open);
RunState serde roundtrip incl. `Conversation`+`GoalState`; v9 migration + run-state
CRUD + reject injects declined response; WS: ready advertises caps, needs_approval
emits `approval{}`, approve resumes to `done`, PII-block emits `guardrail{blocked:true}`.

---

## 4. Cluster 3 — Orchestration · Structured outputs · Reliability · Model surface

### 4.1 Multi-agent orchestration

Generalize the knowledge sub-agent loop (don't move it): `crates/biorouter/src/
agents/subagent/mod.rs` re-exports `SubAgent`/`Completer`/`ToolDispatch`/
`SubAgentBounds` and the existing `ProviderCompleter` bridge; adds `NamedSubAgent
{name,description,system_prompt,tools,bounds,input_schema}` and
`ExtensionToolDispatch` (routes a sub-agent's tool calls back through the parent
`ExtensionManager`, deny-by-default `allowed` set).

- **Agents-as-tools**: store `sub_agents: Mutex<HashMap<String,Arc<NamedSubAgent>>>`
  on `Agent`; push one `agent__<name>` tool per entry in `list_tools`
  (`agent.rs:872`); add a dispatch branch in `dispatch_tool_call` (`agent.rs:617`,
  beside `SUBAGENT_TOOL_NAME`) that runs `SubAgent::run` with the app's provider.
  Recursion guard mirrors the existing one (`agent.rs:557`).
- **Handoffs**: `orchestration.agents` are alternate `AgentConfig`s. Track
  `active_agent_id` in `handle_agent_socket`; client `handoff{to}` or a model-
  called `handoff` tool (sentinel result) triggers `Agent::reset_for_handoff()` +
  re-`configure_agent` on the same session (conversation preserved). Emit
  `handoff{from,to}`.
- **Lazy tools** (`agents/lazy_tools.rs`): when `orchestration.lazy_tools`,
  `list_tools` returns core + a `tool_search` meta-tool; `tool_search(query)`
  activates matches and sets `tools_updated=true` so the existing refresh path
  (`agent.rs:1712`) reloads — no new loop machinery.
- **Workflows** (`agents/app_workflow.rs`): `WorkflowManifest` steps
  (`Tool{tool,args_template,guardrail,on_error}` / `Agent{agent,input_template,…}`),
  run on `workflow_run{name,input}`, each step validated against its guardrail
  schema (reuse §4.2), `on_error ∈ {abort,continue,retry(n),fallback(step)}`,
  streaming per-step frames.

### 4.2 Structured outputs

`SessionConfig` (`agents/types.rs:84`) gains `output_type: Option<Value>` +
`output_retries: Option<u32>`. New `agents/structured_output.rs`:
`validate(value, schema)` (crate `jsonschema`). At natural completion
(`agent.rs:1717-1778`), if `output_type` set: strip ```json fences (like
`create_workflow`, `agent.rs:2124`), validate; on success set `exit_chat` + emit
`AgentEvent::Output{schema,value}`; on failure with attempts left, push a
corrective user message (errors listed) and continue the loop (bounded by
`output_retries`). Maps to the `output{}` frame; `br.run(p,target,{outputType})`
resolves with the typed object and skips markdown scraping.

### 4.3 Reliability cluster

`SessionConfig.reliability: Option<ReliabilityConfig>`:
```rust
pub struct ReliabilityConfig { pub tool_timeout_s:Option<u64>, pub tool_timeout_behavior:ToolTimeoutBehavior,
  pub tool_not_found_behavior:ToolNotFoundBehavior, pub tool_use_behavior:ToolUseBehavior,
  pub error_to_output:bool, pub parallel_tools:bool, pub reset_tool_choice:bool }
```
Mapped to real sites: **timeout** wraps the awaited tool future in
`dispatch_tool_call` (`agent.rs:660`) → `ErrorAsResult` returns an `is_error`
`CallToolResult` (model self-corrects) or `Raise`; **concurrency** spawns each
tool future via `tokio::spawn` when set (the `select_all` at `agent.rs:1473` is
already concurrent for IO); **not-found** synthesizes a non-fatal result
(`agent.rs:668`); **stop conditions** set `exit_chat` in the post-tool path
(`agent.rs:1712`); **error→output** (`agents/reliability.rs::graceful_finalize`)
turns max_turns/refusal/provider-error breaks (`agent.rs:1277`/`:1701`/`:1833`)
into one bounded summarizing completion; **reset_tool_choice** is bookkeeping now,
full provider `tool_choice` plumbing is phased (§10).

### 4.4 Per-app model surface

`ModelSelection.settings: ModelSettings{temperature,max_tokens,top_p,reasoning_effort,verbosity}`
→ in `configure_agent` (`apps.rs:250`) map onto `ModelConfig` (`model.rs:140`):
`temperature`/`max_tokens` first-class, the rest into `request_params` (the
existing provider-agnostic carrier; `openai.rs:709` already reads
`reasoning_effort`; add `top_p`/`verbosity` to provider payloads). `model_select`
frame → `agent.update_provider(p, session_id)` (`agent.rs:1934`). Extend `Usage`
(`providers/base.rs:310`) with `cached_tokens`/`reasoning_tokens` (defaults;
update `Add`/`AddAssign`); emit `AgentEvent::Usage` per turn → `usage{}` frame;
`br.context.usage()` accumulates. `br.model.list()` via new `GET /apps/{id}/models`
(filtered `providers()`).

**Test plan**: sub-agent deny-by-default dispatch; `agent__*` in list_tools +
recursion guard; structured-output validate/fence-strip/bounded re-prompt/
graceful_finalize; reliability (timeout→is_error; not-found non-fatal;
stop_on_first_tool; parallel wall-clock≈max); lazy-tools (core-only → activate →
reduced `count_tokens_for_tools`); model settings→ModelConfig; Usage Add incl new
fields; manifest serde back-compat; WS variant roundtrips.

---

## 5. Cluster 4 — Context recovery/compaction/sharing · Tracing · Lifecycle

### 5.a Durable, resumable sessions (the recovery fix)

`session_id` keyed by a browser-persisted `client_id` (localStorage
`br.client.<app_id>`), sent as `?client_id=…` on the WS URL → `external_key =
"app:{app_id}:{client_id}"`. v9 adds the `external_key` column +
`get_or_create_by_external_key(...)`. `handle_agent_socket` (`apps.rs:347`)
resolves-or-creates by external_key; `get_agent(session_id)` (`state.rs:106`)
reuses the cached agent so `reply()` resumes from the stored conversation. `ready`
carries `{sessionId, clientKey, resumed, messageCount}`; new `GET /apps/{id}/session/{client_id}/messages`
returns `conversation.user_visible_messages()` for the SDK to repaint on resume.

### 5.b Compaction surfaced

`br.context.{tokens,compact,snapshot,restore}` over frames `tokens`/`compact`/
`snapshot`/`restore` ↔ `context{used,limit,ratio}`/`compaction{phase,trigger,
before,after}`. Reuse: `tokens` ← `get_token_counts` (`session_manager.rs:312`) +
`get_model_config().context_limit()`; `compact` ← the manual-compact path
`handle_compact_command` (`execute_commands.rs:99`) → `compact_messages(provider,
conv, true)` (`context_mgmt/mod.rs:50`). Auto-compaction already runs in `reply()`
(`agent.rs:1129`); the route forwards `compaction{}` from the ObsBus and the
existing `HistoryReplaced` event tells the app to refetch backlog. *Optional*
progressive-disclosure mode: write each compaction summary + pre-compaction
archive to `knowledge/_context/<session>/…` via `write_page`, add
`br.context.recall(query)` (scoped BM25) — ship after the core path.

### 5.c Context sharing (KB-backed)

```rust
pub struct MemoryCapability { pub kb:Option<String>, pub mode:MemoryMode /*off|read|read_write*/,
  pub shared_kb:Option<String>, pub distill:bool }
```
Within-app scratch: `br.memory.{note,recall}` → `write_page`/scoped `search`
under `knowledge/scratch/{app_id}/`. **Two-phase distillation** (`apps/distill.rs`)
on session end (WS read-loop exit, detached `tokio::task` since the session is now
durable; also a `SessionEnd` hook): Phase 1 extract lessons via
`provider.complete_fast` over agent-visible messages (reuse
`format_message_for_compacting`); Phase 2 `write_page("knowledge/lessons/{app}/{date}-{session}.md")`
(git + BM25 free; idempotent). Cross-app: `shared_kb` id → writes under
`knowledge/shared/{app_id}/` in that KB so apps sharing an id read each other's
pages; per-KB write `Mutex` to serialize git commits (confirm one doesn't exist).
`read` mode rejects writes.

### 5.d Tracing

New `crates/biorouter/src/observability/`: a process-global `ObsBus`
(`tokio::sync::broadcast<(session_id, ObsEvent)>`) published next to each existing
hook fire (`agent.rs:262`/`:1526`/`:986`/`:1798`, `subagent_handler.rs:140`) —
observe-only, drops if no subscriber. `ObsEvent` covers turn/tool/llm/handoff/
compaction/guardrail/session. A per-session `TraceBuilder` folds events into a
`Span{id,parent,kind,name,start_ms,end_ms,status,attrs}` tree; **redacted by
default** (`tracing.redact=true` ⇒ names+timings only, no args/text). Persist in
`Session.extension_data` under `"trace"`. `TraceProcessor` trait for optional
Langfuse/Phoenix/OTLP (feature-gated; in-GUI path needs no external dep). WS route
adds a `tokio::select!` arm draining `ObsBus.subscribe()` filtered by session →
`trace{span}` frames (+ `trace{snapshot}` on resume).
```rust
pub struct TracingCapability { pub enabled:bool, pub redact:bool /*default true*/, pub processor:Option<String> }
```

### 5.e Lifecycle-hook exposure

`ObsEvent` → app frames gated by `capabilities.events`: `tool` (now real
name+id, fixing the `apps.rs:453` placeholder), `handoff`, `llm{phase,model,
tokens}`, `session{phase}`, `compaction`. SDK adds the kinds to the event map;
`handleFrame` already emits by `type`, so `br.on('llm'|'session'|…)` works once
listed.

**Test plan**: `get_or_create_by_external_key` returns same id twice (resume); v9
idempotent; `TraceBuilder` nested-span correctness + redaction; compaction lowers
tokens; distillation writes a findable `lessons/` page; shared-kb cross-read;
read-mode write rejected; `ready` serialization + backlog route; resume reuses
agent.

---

## 6. Cluster 5 — TS client · Interactive widgets · GUI

### 6.a Client refactor (one IIFE, back-compatible)

`sdk.ts` keeps every public member; adds namespace properties + a **frame-dispatch
table** in the constructor (plain object of arrow fns — stripper-safe). `handleFrame`
emits a universal `frame` event, runs the typed handler (or forwards unknown
frames by name), then settles turn lifecycle on `done`/`error`. `br.has(cap)`
gates every new method; against a v1 server (no capabilities) new methods
no-op/reject and old apps are unaffected. New types are `export interface`/`type`
only; generics confined to signatures.

### 6.b Interactive widgets

Widget tree (terse discriminator `t`, every node maps 1:1 to existing `br-*`
theme classes ⇒ **zero new CSS**, passes `lint_app`):
```ts
type WidgetNode =
 | {t:"card";title?;children} | {t:"row";children} | {t:"col";children}
 | {t:"text";value;markdown?;muted?} | {t:"badge";value}
 | {t:"table";columns;rows} | {t:"chart";spec}      // reuses renderChart
 | {t:"input";name;label?;value?;inputType?} | {t:"select";name;options;value?}
 | {t:"checkbox";name;label?;checked?} | {t:"form";children}
 | {t:"button";label;action;variant?;submit?;payload?};
```
**Emission = a `render_widget` MCP tool** (autovisualiser-style: validate +
escape, `common.rs` pattern), whose result `apps.rs` translates into a
`widget{id,tree}` frame — chosen over a structured-output contract because it
keeps emission provider-agnostic (any tool-calling model), streams, allows
multiple widgets/turn, and aligns with the action→tool_result round-trip.
Renderer `renderWidget(node, ctx)` builds DOM via `createElement`+`textContent`
(no `innerHTML` except the already-escaping `renderMarkdown`/`renderChart`);
unknown `t` → muted placeholder (never throws). **Action round-trip**: a
`submit` button collects `ctx.fields` (name→getter) → `widget_action{widgetId,
action,payload}` → server feeds it back as the next user turn *or* resolves a
pending tool (open Q) → agent continues, may emit a replacement widget.

### 6.c App-defined tools

`defineTool(name, schema, handler)` registers into `appTools`; on `ready` (caps
known) the client sends one `register_tools{tools}` frame (the single new
client→server frame beyond the backbone — sent only if server advertises
`tools`); server registers them as dynamic session tools. `tool_call{id,name,args}`
→ `runAppTool` executes the handler → `tool_result{id,value|error}`. These flow
through the same `guardrail{}`/`approval{}` path as built-in tools (server-enforced).

### 6.d GUI

`AppCapabilityBadges.tsx` (capability/guardrail chips on each card, pure render
from manifest JSON), `ApprovalPrompt.tsx` (consumes `approval{}`), `TraceTimeline.tsx`
(Gantt from `trace{}` spans), `useAppAgentSocket.ts` (opens the per-app WS from
the GUI for in-GUI hosting; browser-launch path unchanged). Reuses the existing
warm-beige Tailwind tokens; `useChatStream.ts` untouched.

**Test plan** (vitest via alias to the real `sdk.ts`, jsdom + `MockWebSocket`):
frame dispatch incl. unknown-forward; back-compat `on('message')`+`prompt`
resolve/reject; capability gating; `defineTool` roundtrip (register_tools →
tool_call → tool_result, incl. error); `renderWidget` per node + escaping + form
collection; approve/reject/compact/snapshot/model_select map to exact frames;
renderMarkdown/renderChart snapshots locked. Rust: `bundle.rs` asserts the new
`dist/app.js` contains `widget_action`/`register_tools` (survives esbuild + the
fallback stripper) and that `strip_module_syntax(sdk.ts)` keeps `renderWidget`/
`defineTool` with no `interface`/`import`.

---

## 7. Cross-cluster conflict resolutions

1. **One `capabilities` block.** Two clusters added `capabilities` with different
   shapes → unified in §1.2 (`files/data/compute/vault/memory/tracing/events`).
   `guardrails`, `reliability`, `orchestration`, `output_type`, `durable_session`
   are siblings, not under `capabilities`.
2. **Transport for `br.files` vs `br.data`/`br.compute`.** Files = **signed HTTP
   routes** (bulk bytes off the WS — Cluster 1). Data/compute = **dedicated
   additive WS request/response frames** (`data_query`/`data_result`,
   `compute_run`/`compute_result`). Cluster 5's `output{__reqId}` envelope is the
   fallback only if we later want a generic client-RPC; the typed TS surface is
   identical either way.
3. **One v9 migration** carries both `external_key` and `agent_run_states` (§1.7).
4. **`SandboxClient` trait placement** (dependency direction: `biorouter` →
   `biorouter-mcp` exists today, and the consumers `developer`/`computercontroller`
   live in `biorouter-mcp`). Resolution: introduce a tiny leaf crate
   `biorouter-sandbox` for the trait (both crates depend on it), or place the
   trait in `biorouter-mcp` and re-export a `create()` factory from
   `biorouter/src/sandbox/`. **Decision needed** — recommend the leaf crate
   (cleanest, no cycle).
5. **`register_tools`** is the one sanctioned new client→server frame beyond the
   backbone set (app-tool advertisement on connect).
6. **`render.rs::app_config_script`** must be extended (Cluster 1/5) to inject
   `capabilities` + signed file token into `window.BIOROUTER_APP_CONFIG`; the
   client consumes them.

---

## 8. Phased build order (dependency-ordered, shippable per phase)

- **Phase 0 — Foundation.** Unified manifest (§1.2) + WS v2 frame scaffolding +
  `br` namespaced refactor with full back-compat + the `ready` capability list +
  deny-by-default gating skeleton. No behavior yet; everything green. (Cluster 5
  client core + §1.)
- **Phase 1 — Durable sessions + context API.** v9 `external_key`, resume,
  `br.context.tokens/compact` + `compaction` events. *Delivers "recover what it
  lost."* (Cluster 4a/b.)
- **Phase 2 — Reliability + structured outputs + content guardrails.** Reliability
  cluster, `output_type`, the guardrail pipeline (PII/PHI + injection +
  groundedness + moderation) and the `guardrails.goal` one-liner. *The
  correctness/safety headline.* (Cluster 3b/c + Cluster 2a/b.)
- **Phase 3 — Files + Data.** Workspace jail, signed file routes, bundled DuckDB
  MCP. (Cluster 1.1/1.2.)
- **Phase 4 — Sandboxed compute + vault.** `SandboxClient` + Docker backend; per-app
  encrypted vault. *Security keystone.* (Cluster 1.3/1.4.)
- **Phase 5 — HITL + RunState.** v9 `agent_run_states`, approvals, serializable
  resume, crash recovery. (Cluster 2c.)
- **Phase 6 — Orchestration + app tools + widgets.** Agents-as-tools, handoffs,
  lazy tools, workflows; `defineTool`; interactive widgets. (Cluster 3a + Cluster
  5b/c.)
- **Phase 7 — Tracing + model surface + memory + GUI.** ObsBus span tree + GUI
  timeline; per-app model surface + usage; KB memory + distillation; capability/
  guardrail/approval GUI. (Cluster 4d/e + 3d + 5d.)

Each phase: `cargo test -p {biorouter,biorouter-mcp,biorouter-server}` + (where TS
touched) `cd ui/desktop && npm run test:run`; after route changes
`just generate-openapi && cd ui/desktop && npm run generate-api`.

---

## 9. Consolidated test strategy

- **Unit (Rust)**: jail, PII detector, guardrail pipeline, RunState serde,
  structured-output validate, reliability behaviors, sub-agent dispatch,
  TraceBuilder, sandbox arg-builder, vault crypto, Usage math.
- **Migration**: v8→v9 idempotent; `create_schema` includes the new column+table;
  external_key unique; run-state CRUD.
- **MCP/server integration**: WS v2 frame roundtrips; needs_approval→approval→
  approve→done; PII-block→guardrail; durable resume; signed file routes
  (401/403/200); DataSql read-only + jail; structured `output{}`; `usage{}`;
  handoff swap.
- **Security**: jail-escape (`..`/symlink/cross-app), path traversal, vault
  plaintext never in frames, Docker isolation flags, signed-token forgery/expiry.
- **TS (vitest, real `sdk.ts` via alias)**: dispatch, back-compat, capability
  gating, defineTool roundtrip, widget render + action collection, frame mapping.
- **Stripper guard**: `strip_module_syntax(sdk.ts)` keeps the new API and emits no
  `interface`/`import`.
- **Live (debug-app skill / Playwright)**: reload mid-conversation resumes;
  widget form round-trips into the loop; trace timeline renders; PII masking
  visible.

---

## 10. Consolidated open questions / decisions

1. **`SandboxClient` crate placement** (§7.4) — leaf crate `biorouter-sandbox`
   (recommended) vs trait in `biorouter-mcp`.
2. **Docker fallback** — if `compute.sandbox=docker` and Docker is absent:
   hard-fail the capability (recommended, preserves deny-by-default) vs silent
   downgrade to `local(unsafe)`.
3. **`tool_choice` depth** — full provider-level `tool_choice` (needed for true
   `reset_tool_choice`) is net-new across `providers/factory.rs`; phase it (loop
   bookkeeping now, provider plumbing later)? Which providers must support forced
   choice for v1?
4. **Output-guardrail timing** — block-as-post-stream-retraction (proposed) vs
   buffer the final answer before flushing (changes latency/UX).
5. **Widget action semantics** — feed `widget_action` back as a new user turn vs
   resolve a pending tool (affects the SDK's `runChain` serialization).
6. **Multi-tab durable sessions** — two tabs share one `client_id` → one shared
   live session (acceptable?) vs tab-scoped suffix.
7. **Trace persistence size** — `extension_data` blob (simple, unbounded) vs a
   dedicated `spans` table (v10) / last-N-turns cap.
8. **Cross-app KB write lock** — confirm `KnowledgeService` serializes concurrent
   `write_page` git commits per KB; add a per-KB `Mutex` if not.
9. **Interactive detection for HITL persistence** — manifest flag vs a
   client-advertised non-interactive capability on connect.
10. **Sub-agent model override** — inherit the app provider (recommended,
    cost-control) vs allow `sub_agents.<name>.model`.
11. **Groundedness crate dependency** — may `biorouter` depend on
    `biorouter-mcp::knowledge::store` BM25, or pass search in as a closure to
    avoid the dependency?

---

### Appendix — primary `file:line` anchors (verified)

`agents/agent.rs`: reply loop `1245`, per-turn loop `1257`, max_turns `1277`,
dispatch `549`/`617`/`660`, list_tools `872`, tool-result drain `1487`, Stop/goal
`1811`, natural-completion `1717-1778`, AgentEvent enum `163`, update_provider
`1934`, compaction `1129`, fire_compaction_hook `262`. `agents/types.rs`:
SessionConfig `84`. `agents/goal.rs`: set_goal `268`, GOAL_MAX_ITERATIONS.
`tool_inspection.rs`: trait `35`, manager build `agent.rs:284`. `hooks/prompt_runner.rs`:
run_prompt_hook `45`. `session/session_manager.rs`: schema `21`, get_session `304`,
get_token_counts `312`, create_schema `709`, migrations `947`.
`context_mgmt/mod.rs`: compact_messages `50`, check_if_compaction_needed `168`.
`providers/base.rs`: Usage `310`, complete_fast `420`. `model.rs`: ModelConfig `140`.
`routes/apps.rs`: ClientFrame `214`, configure_agent `235`, handle_agent_socket
`347`, ready `376`, event relay `428`, routes `480`, DEFAULT_MAX_TURNS `50`.
`agent_drafter/store.rs`: AgentConfig `54`, Manifest `88`. `agent_drafter/bundle.rs`:
lint_app `43`, run_esbuild `285`, strip_module_syntax `333`. `templates/sdk.ts`:
events `44`, listeners `74`, handleFrame `149`, renderMarkdown `432`, renderChart
`318`, mountChat `561`, createApp `655`. `developer/rmcp_developer.rs`: resolve_path
`1545`, execute_shell_command `1229`. `config/base.rs`: set_secret `845`.
`biorouter-mcp/src/lib.rs`: BUILTIN_EXTENSIONS `53`.
