//! BRSDK manifest types — the declarative, **deny-by-default** capability /
//! guardrail / reliability / orchestration surface an Agent-Drafter app uses to
//! switch on engine features.
//!
//! Every field defaults, so manifests written before BRSDK existed deserialize
//! unchanged (absence of a capability = that capability denied). These types are
//! the data model only; each cluster's *behavior* is wired in its own phase:
//! - `Capabilities.{files,data,compute,vault}` — Phases 3–4
//! - `Capabilities.{memory,tracing,events}` — Phases 5/7
//! - `GuardrailsConfig` — Phase 2
//! - `ReliabilityConfig`, `output_type` — Phase 2
//! - `Orchestration` — Phase 6
//! - `durable_session` — Phase 1

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::store::AgentConfig;

fn yes() -> bool {
    true
}

// ───────────────────────────── Model settings ─────────────────────────────

/// Provider-agnostic generation settings an app may expose (Phase 4 consumes).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ModelSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<String>,
}

// ───────────────────────────── Capabilities ───────────────────────────────

/// Deny-by-default capability grants. The absence of a field means the app
/// gets none of that capability.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<FilesCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<DataCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compute: Option<ComputeCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<VaultCapability>,
    #[serde(default)]
    pub memory: MemoryCapability,
    #[serde(default)]
    pub tracing: TracingCapability,
    /// Agent-driven UI control (`ui_*` tools). Unlike the other capabilities this
    /// one is **on by default**: it is confined to the app's own page — the agent
    /// can only mutate the DOM it already owns, and reaches nothing outside the
    /// browser tab. Set `{"enabled": false}` to make an app text-only.
    #[serde(default)]
    pub ui: UiCapability,
    /// Lifecycle events the app may receive via `br.on()`
    /// (e.g. `["tool","handoff","llm","session","compaction"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
}

impl Capabilities {
    /// Capability tokens advertised to the client in the `ready` frame, so old
    /// apps ignore frames they don't understand and new apps can feature-detect.
    pub fn advertised(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.files.is_some() {
            v.push("files".to_string());
        }
        if self.data.is_some() {
            v.push("data".to_string());
        }
        if self.compute.is_some() {
            v.push("compute".to_string());
        }
        if self.vault.is_some() {
            v.push("vault".to_string());
        }
        if self.memory.mode != MemoryMode::Off {
            v.push("memory".to_string());
        }
        if self.tracing.enabled {
            v.push("tracing".to_string());
        }
        if self.ui.enabled {
            v.push("ui".to_string());
        }
        for e in &self.events {
            v.push(format!("event:{e}"));
        }
        v
    }
}

/// Agent-driven UI control. The agent gets `ui_*` tools that push commands down
/// the app's own WebSocket, so it can build panels/dashboards, draw charts,
/// highlight regions, restyle, and ask the user structured questions — instead of
/// only emitting text. Scoped entirely to the app's page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiCapability {
    /// Grant the `ui_*` tools. Default: true (see [`Capabilities::ui`]).
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Allow `ui_theme` to restyle the app (accent, mode, density).
    #[serde(default = "yes")]
    pub allow_theme: bool,
    /// Allow `ui_layout` to switch the page's region layout.
    #[serde(default = "yes")]
    pub allow_layout: bool,
    /// Allow `ui_ask` to block a tool call on a user form submission.
    #[serde(default = "yes")]
    pub allow_ask: bool,
    /// Allow `ui_subscribe` — the agent may subscribe to app→agent signals the
    /// author declared in `surface.signals`.
    ///
    /// **Default: `true`.** Unlike raw HTML, subscribing is safe to grant by
    /// default: the token cost is bounded by each signal's `coalesce_ms`
    /// (server-side rate cap) plus the payload/size caps enforced when a signal is
    /// validated, so a chatty app cannot flood the agent. The genuinely risky
    /// autonomy switch — letting a signal auto-run a turn on its own (autorun) — is
    /// a separate capability that lands in a later phase; this flag only governs
    /// whether the agent may *listen*.
    #[serde(default = "yes")]
    pub allow_signals: bool,
    /// Allow `ui_html` to inject server-sanitized rich HTML into the page.
    ///
    /// **Default: `false`** — deliberately unlike the other `allow_*` switches,
    /// which default on. Raw HTML is a real XSS surface (design §3.7): even
    /// though `ui_html` sanitizes fail-closed server-side (in `control.rs`, so
    /// the frame never leaves the daemon unsanitized), the sanitizer then *is* a
    /// primary injection barrier. An app must therefore opt into it explicitly
    /// rather than inherit it — the whole point of a capability. Off ⇒ `ui_html`
    /// is denied and the agent is told so.
    #[serde(default)]
    pub allow_html: bool,
    /// Allow app→agent signals to *autonomously start a turn* (autorun, design
    /// §3.5/§3.7).
    ///
    /// **Default: `false`** — the same deny-by-default posture as `allow_html`,
    /// and for a stronger reason: a signal-triggered turn spends the user's
    /// provider quota without a human in the loop, which on commercial or
    /// institutional providers is real money. `allow_signals` only lets the agent
    /// *listen*; this flag lets a signal *act*. It is user-granted only (the agent
    /// can never self-grant), a signal must additionally opt in via its
    /// [`SignalDecl::autorun`] flag, and the server enforces per-minute/per-session
    /// budgets. Off ⇒ every signal stays queue-only.
    #[serde(default)]
    pub allow_autorun: bool,
    /// Cap on simultaneously mounted agent panels (oldest evicted past this).
    #[serde(default = "default_max_panels")]
    pub max_panels: usize,
    /// Seconds a `ui_ask` waits for the user before returning a timeout result.
    #[serde(default = "default_ask_timeout_s")]
    pub ask_timeout_s: u64,
}

fn default_max_panels() -> usize {
    12
}
fn default_ask_timeout_s() -> u64 {
    300
}

impl Default for UiCapability {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_theme: true,
            allow_layout: true,
            allow_ask: true,
            allow_signals: true,
            // Off by default even though every sibling defaults on — see the
            // field docs: raw HTML is an XSS surface, so it is opt-in.
            allow_html: false,
            // Off by default: autonomous, quota-spending turns are user-granted
            // only (see the field docs).
            allow_autorun: false,
            max_panels: default_max_panels(),
            ask_timeout_s: default_ask_timeout_s(),
        }
    }
}

/// A host directory mounted into the app workspace.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    /// Mount name exposed to the app (e.g. "data", "out").
    pub name: String,
    /// Absolute host dir to mount; `None` → a subdir of the app workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_dir: Option<String>,
    /// "ro" | "rw".
    #[serde(default = "ro_mode")]
    pub mode: String,
    /// An empty output dir created for the app to write into.
    #[serde(default)]
    pub out_dir: bool,
}
fn ro_mode() -> String {
    "ro".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilesCapability {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<WorkspaceEntry>,
    /// Cap on a single read/write/upload (server-enforced).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_file_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSource {
    pub name: String,
    /// "knowledge" | "spoke" | "omop" | "cdw" | "sql"
    pub kind: String,
    /// For kind="sql": workspace-relative db file (DuckDB/SQLite).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// For extension-backed sources: the extension / KB id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<String>,
    /// The **specific** ids this source scopes the app to (design §3.4). For
    /// `kind="knowledge"` these are the knowledge-base id(s) the app may touch —
    /// an app is NEVER granted "all bases". Two consequences the `br.kb`
    /// handler enforces:
    /// - empty `ids` + `kind="knowledge"` grants **NOTHING** by itself (the kb
    ///   ops reply with an error that explains this) — the only exception is the
    ///   back-compat implicit single grant of the agent's configured
    ///   `knowledge_base`, if one is set.
    /// - a KB id that is not enumerated here is denied, even if it exists.
    #[serde(default)]
    pub ids: Vec<String>,
    /// `true` (default) = read-only. Setting it `false` grants **write** access
    /// (e.g. `br.kb.ingest`), which is a *separately and prominently consented*
    /// decision, not a checkbox (design §3.4): a poisoned ingest persists in a
    /// git-backed KB that other sessions and agents read, so write access is a
    /// cross-session integrity decision. The `br.kb` handler therefore requires
    /// `read_only == false` on the granting knowledge source before it will run
    /// an `ingest`.
    #[serde(default = "yes")]
    pub read_only: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DataCapability {
    #[serde(default)]
    pub sources: Vec<DataSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeCapability {
    /// "none" | "local" | "docker"
    #[serde(default = "sandbox_none")]
    pub sandbox: String,
    #[serde(default = "default_timeout_s")]
    pub timeout_s: u64,
    /// "none" | "host" (docker --network)
    #[serde(default = "net_none")]
    pub network: String,
    /// e.g. "512m"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_mem: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpus: Option<f64>,
    /// Docker image (default: a pinned python image).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}
fn sandbox_none() -> String {
    "none".to_string()
}
fn net_none() -> String {
    "none".to_string()
}
fn default_timeout_s() -> u64 {
    60
}
impl Default for ComputeCapability {
    fn default() -> Self {
        Self {
            sandbox: sandbox_none(),
            timeout_s: default_timeout_s(),
            network: net_none(),
            max_mem: None,
            cpus: None,
            image: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VaultCapability {
    /// Secret names referenceable via `{{vault:NAME}}`.
    #[serde(default)]
    pub encrypted: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryMode {
    #[default]
    Off,
    Read,
    ReadWrite,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryCapability {
    /// KB id used for scratch memory + distillation output. `None` → reuse
    /// `AgentConfig::knowledge_base` if set, else memory is off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kb: Option<String>,
    #[serde(default)]
    pub mode: MemoryMode,
    /// Cross-app shared KB id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_kb: Option<String>,
    /// Run the two-phase distill-lessons job at session end.
    #[serde(default)]
    pub distill: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingCapability {
    #[serde(default)]
    pub enabled: bool,
    /// Redact tool args/results & message text from spans (names + timings
    /// only). Defaults to true → sensitive-data-safe by default.
    #[serde(default = "yes")]
    pub redact: bool,
    /// Optional external processor: "langfuse" | "phoenix" | "otlp".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub processor: Option<String>,
}
impl Default for TracingCapability {
    fn default() -> Self {
        Self {
            enabled: false,
            redact: true,
            processor: None,
        }
    }
}

// ───────────────────────────── Guardrails ─────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PiiMode {
    #[default]
    Off,
    Mask,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailCheck {
    /// "injection" | "groundedness" | "moderation"
    pub kind: String,
    /// Override default stages (e.g. ["output"]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<String>,
    /// "block" | "fail" | "warn"
    #[serde(default = "block_trip")]
    pub on_trip: String,
}
fn block_trip() -> String {
    "block".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GuardrailsConfig {
    /// One-line goal → auto-installs the goal Stop-hook for the session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    /// Off-topic / moderation scope for the LLM-judge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub business_scope: Option<String>,
    #[serde(default)]
    pub pii: PiiMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<GuardrailCheck>,
    /// Tools that always require human approval.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs_approval: Vec<String>,
    /// Persist a RunState + emit `approval{}` when no interactive UI is present.
    #[serde(default)]
    pub approvals_require_persistence: bool,
}

// ───────────────────────────── Reliability ────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolTimeoutBehavior {
    #[default]
    ErrorAsResult,
    Raise,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolNotFoundBehavior {
    #[default]
    ReturnErrorToModel,
    Raise,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ToolUseBehavior {
    #[default]
    RunLlmAgain,
    StopOnFirstTool,
    StopAtTools {
        names: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_timeout_s: Option<u64>,
    #[serde(default)]
    pub tool_timeout_behavior: ToolTimeoutBehavior,
    #[serde(default)]
    pub tool_not_found_behavior: ToolNotFoundBehavior,
    #[serde(default)]
    pub tool_use_behavior: ToolUseBehavior,
    #[serde(default)]
    pub error_to_output: bool,
    #[serde(default)]
    pub parallel_tools: bool,
    #[serde(default = "yes")]
    pub reset_tool_choice: bool,
}
impl Default for ReliabilityConfig {
    fn default() -> Self {
        Self {
            tool_timeout_s: None,
            tool_timeout_behavior: ToolTimeoutBehavior::default(),
            tool_not_found_behavior: ToolNotFoundBehavior::default(),
            tool_use_behavior: ToolUseBehavior::default(),
            error_to_output: false,
            parallel_tools: false,
            reset_tool_choice: true,
        }
    }
}

// ───────────────────────────── Orchestration ──────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubAgentManifest {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_wall_s: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowStep {
    Tool {
        tool: String,
        #[serde(default)]
        args_template: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        guardrail: Option<serde_json::Value>,
        #[serde(default = "abort_on_err")]
        on_error: String,
    },
    Agent {
        agent: String,
        #[serde(default)]
        input_template: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        guardrail: Option<serde_json::Value>,
        #[serde(default = "abort_on_err")]
        on_error: String,
    },
}
fn abort_on_err() -> String {
    "abort".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowManifest {
    #[serde(default)]
    pub steps: Vec<WorkflowStep>,
}

/// A named model **route** (design §3.4 `agent.routes`): a light provider/model
/// profile (`"fast"`, `"deep"`, `"local_only"`, …) that a `call` / `br.call`
/// can select per invocation. Unlike an orchestration *agent profile* (a full
/// [`AgentConfig`]), a route only redirects which provider/model answers the
/// turn — it carries no separate prompt/extensions/skills.
///
/// Both fields are optional: an absent field inherits the session's current
/// value (so `{"model": "…"}` keeps the provider and swaps the model, and
/// `{"provider": "…"}` keeps the model and swaps the provider). Routes resolve
/// against the *user's* configured providers only — apps never carry keys — and
/// are subject to the provider-class constraint (design §3.7): an app holding a
/// sensitive data source (`omop`/`cdw`, or a `knowledge` source with
/// `read_only == false`) cannot route that data to an external commercial
/// provider.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRoute {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Multi-agent orchestration: sub-agents-as-tools, handoff targets, and
/// declarative workflows. Handoff targets are full `AgentConfig`s (recursion is
/// bounded by the author and the empty-by-default maps).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Orchestration {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub sub_agents: HashMap<String, SubAgentManifest>,
    /// Named **worker agent profiles** (design §3.8, Phase 4b). Each is a full
    /// alternate [`AgentConfig`] — its own model, system prompt, extensions and
    /// KB — that a `prompt` / `call` frame can target via `"agent": "<name>"`, or
    /// that the main agent can reach with the `consult` tool. The app socket loop
    /// validates each profile at connect (capability subset of the app, resolvable
    /// provider/model, provider-class constraint), caps the count, and advertises
    /// the survivors in the `ready.profiles` list. Empty ⇒ a single-agent app.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub agents: HashMap<String, AgentConfig>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub workflows: HashMap<String, WorkflowManifest>,
    /// Named model routes (design §3.4 `agent.routes`) a `call`/`br.call` may
    /// select per invocation. Homed here (rather than as a bare `AgentConfig`
    /// field) so the whole model-profile surface lives in one manifest module;
    /// the manifest path is `agent.orchestration.routes`. Empty ⇒ no routes,
    /// and a `call` without a `route` runs on the session's default provider.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub routes: HashMap<String, ModelRoute>,
    /// Defer tool-schema loading until `tool_search` activates them.
    #[serde(default)]
    pub lazy_tools: bool,
}

// ───────────────────────────── Surface (SDK v2) ────────────────────────────

fn default_coalesce_ms() -> u64 {
    250
}
fn empty_object() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}
fn is_empty_object(v: &serde_json::Value) -> bool {
    v.is_null() || v.as_object().is_some_and(serde_json::Map::is_empty)
}

/// The declared **app contract** (Apps SDK v2, Pillar 1): the typed surface an
/// app exposes to its agent and vice-versa. Every field defaults, so a manifest
/// written before this block existed deserializes exactly as a v1 manifest did
/// (an absent `surface` is indistinguishable from an empty one).
///
/// Only the shape is defined here; the behavior lands per phase:
/// - `state_schema` — the shared state document's JSON Schema (Pillar 2, Phase 1;
///   validated server-side when present, default structural caps otherwise).
/// - `actions` — app verbs the agent may call via `app_call` (Pillar 1, Phase 3).
/// - `signals` — app→agent notifications the agent may subscribe to (Phase 3).
/// - `components` — custom catalog components the app registers (Pillar 3, Phase 2).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SurfaceDecl {
    /// JSON Schema for the shared state document. When present it is enforced
    /// server-side; when absent the default structural caps still apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_schema: Option<serde_json::Value>,
    /// App-defined verbs the AGENT may invoke (`app_call`). The author registers
    /// handlers in `main.ts`; the SDK enforces that registrations match these
    /// declarations at build/lint time. Declared now, consumed in Phase 3.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ActionDecl>,
    /// App→agent notifications the agent may subscribe to. Declared now,
    /// consumed in Phase 3.
    ///
    /// Deliberately named `signals`, **not** `events`, to avoid colliding with
    /// [`Capabilities::events`], which already exists with the *opposite*
    /// direction: `Capabilities.events` is the agent-lifecycle stream pushed
    /// **to** the app via `br.on()` (advertised as `event:<name>` tokens),
    /// whereas these `signals` flow app **to** agent. Keeping the two names
    /// distinct keeps the two channels independent and unambiguous.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<SignalDecl>,
    /// Custom catalog components the app registers (Pillar 3). Declared now,
    /// consumed in Phase 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<ComponentDecl>,
}

impl SurfaceDecl {
    /// True when nothing is declared — used to keep a v1 manifest from gaining a
    /// `surface: {}` key when re-serialized.
    pub fn is_empty(&self) -> bool {
        self.state_schema.is_none()
            && self.actions.is_empty()
            && self.signals.is_empty()
            && self.components.is_empty()
    }
}

/// An app-defined verb the agent may call (via the `app_call` tool).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// JSON Schema for the action's arguments (`{}` → unconstrained).
    #[serde(default = "empty_object", skip_serializing_if = "is_empty_object")]
    pub params: serde_json::Value,
}

/// An app→agent notification the agent may subscribe to. See [`SurfaceDecl::signals`]
/// for why these are "signals" and not "events".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalDecl {
    pub name: String,
    /// JSON Schema for the signal payload (`None` → unconstrained).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    /// Minimum milliseconds between deliveries of this signal (server-side
    /// coalescing / rate cap).
    #[serde(default = "default_coalesce_ms")]
    pub coalesce_ms: u64,
    /// Whether this signal may *start a turn* on its own (autorun, design §3.5).
    ///
    /// **Default: `false`.** A signal must explicitly opt in to be turn-triggering,
    /// and even then autorun only fires when the app also holds the user-granted
    /// [`UiCapability::allow_autorun`] and the server's autorun budgets hold.
    /// Absent/false ⇒ the signal is queue-only (context for the next turn).
    #[serde(default)]
    pub autorun: bool,
    /// Whether declaring this signal ALSO subscribes the agent to it.
    ///
    /// **Default: `true`.** This is the fix for the worst failure in the 100-app
    /// test drive: signals round-tripped 1 time in 12. The subscription set used
    /// to start empty on every connection and the *only* way to fill it was the
    /// agent voluntarily calling `ui_subscribe` — but the user's first click
    /// necessarily happens *before* the agent's first tool call, so the gesture was
    /// validated against an empty set, rejected, and **dropped**. No prompt can win
    /// an ordering race that happens before any prompt is evaluated; one probe
    /// called `ui_subscribe` five times in a row trying.
    ///
    /// Declaring a signal now *is* subscribing to it. `ui_subscribe` remains, for
    /// adding non-eager signals. Set `eager: false` for a signal the agent should
    /// only receive after explicitly opting in.
    #[serde(default = "yes")]
    pub eager: bool,
}

impl Default for SignalDecl {
    fn default() -> Self {
        Self {
            name: String::new(),
            payload: None,
            coalesce_ms: default_coalesce_ms(),
            autorun: false,
            eager: true,
        }
    }
}

/// A custom catalog component the app registers (Pillar 3).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComponentDecl {
    pub name: String,
    /// JSON Schema for the component props (`{}` → unconstrained).
    #[serde(default = "empty_object", skip_serializing_if = "is_empty_object")]
    pub props: serde_json::Value,
}

// ───────────────────────────── Theme (SDK v2) ──────────────────────────────

/// The curated theme packs an app may select (Apps SDK v2, Pillar 6). Each name
/// corresponds to a `[data-br-pack="<name>"]` token layer in
/// `templates/theme.css`. `biorouter` is the base look (no overrides), so an app
/// that never sets a pack renders exactly like a v1 app.
pub const THEME_PACKS: &[&str] = &[
    "biorouter",
    "clinical",
    "lab-notebook",
    "terminal",
    "journal",
    "midnight",
];

/// The base pack — the historical BioRouter light/dark look.
pub const DEFAULT_THEME_PACK: &str = "biorouter";

fn default_pack() -> String {
    DEFAULT_THEME_PACK.to_string()
}

/// True when a token KEY names a `--br-*` custom property, so an override can
/// only touch the design-system tokens and never inject an arbitrary CSS
/// declaration.
pub fn is_safe_token_key(k: &str) -> bool {
    let rest = match k.strip_prefix("--br-") {
        Some(r) => r,
        None => return false,
    };
    !rest.is_empty()
        && k.len() <= 48
        && rest
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// True when a CSS value is safe to splice into a `--br-*: <value>;`
/// declaration. Mirrors `ui_theme`'s accent sanitizer (reject `;`/`}`/`{`,
/// angle brackets, parentheses — which also blocks `url(`/`color-mix(` — plus
/// quotes/backslash/slash) so an author's token override can never break out of
/// the rule or smuggle a `url(...)` fetch.
pub fn is_safe_token_value(v: &str) -> bool {
    let v = v.trim();
    !v.is_empty()
        && v.len() <= 64
        && !v.contains([';', '}', '{', '<', '>', '(', ')', '"', '\'', '\\', '/'])
}

/// An app's theme selection: a curated pack plus optional accent and custom
/// `--br-*` token overrides. Every field defaults to the base look, so a v1
/// manifest (no `theme` block) deserializes and re-serializes unchanged.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemeConfig {
    /// One of [`THEME_PACKS`]; selects the `[data-br-pack]` token layer. An
    /// unrecognised value falls back to [`DEFAULT_THEME_PACK`] at render time
    /// (see [`ThemeConfig::resolved_pack`]).
    #[serde(default = "default_pack")]
    pub pack: String,
    /// Accent colour override, sanitized like `ui_theme`'s accent. `None` → the
    /// pack's own accent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    /// Custom `--br-*` token overrides. Keys must be `--br-*` custom properties
    /// and values must pass [`is_safe_token_value`]; anything else is dropped at
    /// render time (see [`ThemeConfig::sanitized_tokens`]).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub tokens: HashMap<String, String>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            pack: default_pack(),
            accent: None,
            tokens: HashMap::new(),
        }
    }
}

impl ThemeConfig {
    /// True when nothing is customised — used to keep a v1 manifest from gaining
    /// a `theme: {}` key on re-serialize.
    pub fn is_default(&self) -> bool {
        self.pack == DEFAULT_THEME_PACK && self.accent.is_none() && self.tokens.is_empty()
    }

    /// The selected pack, validated against [`THEME_PACKS`]; an unknown pack
    /// resolves to [`DEFAULT_THEME_PACK`] so a bad manifest can't inject an
    /// arbitrary `[data-br-pack]` attribute value.
    pub fn resolved_pack(&self) -> &str {
        if THEME_PACKS.contains(&self.pack.as_str()) {
            &self.pack
        } else {
            DEFAULT_THEME_PACK
        }
    }

    /// The accent override if it passes the sanitizer, else `None`.
    pub fn sanitized_accent(&self) -> Option<&str> {
        self.accent
            .as_deref()
            .map(str::trim)
            .filter(|a| is_safe_token_value(a))
    }

    /// The token overrides that pass both key and value sanitizers, sorted for a
    /// deterministic render. Unsafe entries are silently dropped.
    pub fn sanitized_tokens(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .tokens
            .iter()
            .filter(|(k, v)| is_safe_token_key(k) && is_safe_token_value(v))
            .map(|(k, v)| (k.clone(), v.trim().to_string()))
            .collect();
        out.sort();
        out
    }

    /// True when the app carries an accent or token override the renderer must
    /// splice in as an extra style layer.
    pub fn has_overrides(&self) -> bool {
        self.sanitized_accent().is_some() || !self.sanitized_tokens().is_empty()
    }
}

#[cfg(test)]
mod theme_tests {
    use super::*;

    #[test]
    fn theme_config_defaults_are_the_base_look() {
        let t = ThemeConfig::default();
        assert_eq!(t.pack, DEFAULT_THEME_PACK);
        assert!(t.is_default());
        assert_eq!(t.resolved_pack(), "biorouter");
        assert!(t.sanitized_accent().is_none());
        assert!(t.sanitized_tokens().is_empty());
        assert!(!t.has_overrides());
    }

    #[test]
    fn theme_config_roundtrips_through_serde_with_defaults() {
        // An absent block deserializes to the default.
        let t: ThemeConfig = serde_json::from_str("{}").unwrap();
        assert!(t.is_default());
        // A customised block round-trips losslessly.
        let src = r##"{"pack":"clinical","accent":"#2563eb","tokens":{"--br-radius":"6px"}}"##;
        let t: ThemeConfig = serde_json::from_str(src).unwrap();
        assert_eq!(t.pack, "clinical");
        assert_eq!(t.accent.as_deref(), Some("#2563eb"));
        let back = serde_json::to_string(&t).unwrap();
        let again: ThemeConfig = serde_json::from_str(&back).unwrap();
        assert_eq!(t, again);
    }

    #[test]
    fn every_pack_name_is_known() {
        for p in THEME_PACKS {
            let t = ThemeConfig {
                pack: (*p).to_string(),
                ..Default::default()
            };
            assert_eq!(t.resolved_pack(), *p);
        }
    }

    #[test]
    fn unknown_pack_resolves_to_base() {
        let t = ThemeConfig {
            pack: "neon-hacker".into(),
            ..Default::default()
        };
        assert_eq!(t.resolved_pack(), "biorouter");
        // A non-base string is still not the default (so it is serialized).
        assert!(!t.is_default());
    }

    #[test]
    fn token_sanitizer_drops_unsafe_keys_and_values() {
        let mut tokens = HashMap::new();
        tokens.insert("--br-radius".to_string(), "4px".to_string()); // ok
        tokens.insert("--br-bg".to_string(), " #fff ".to_string()); // ok (trimmed)
        tokens.insert("color".to_string(), "red".to_string()); // bad key: no --br-
        tokens.insert("--BR-BG".to_string(), "red".to_string()); // bad key: uppercase
        tokens.insert("--br-x".to_string(), "red;}body{color:red".into()); // breakout
        tokens.insert("--br-y".to_string(), "url(http://x)".into()); // url(
        let t = ThemeConfig {
            pack: "biorouter".into(),
            accent: None,
            tokens,
        };
        let safe = t.sanitized_tokens();
        assert_eq!(safe.len(), 2, "only the two safe tokens survive: {safe:?}");
        assert!(safe.iter().any(|(k, v)| k == "--br-radius" && v == "4px"));
        assert!(safe.iter().any(|(k, v)| k == "--br-bg" && v == "#fff"));
        // sorted, deterministic order
        assert!(safe.windows(2).all(|w| w[0].0 <= w[1].0));
    }

    #[test]
    fn accent_sanitizer_matches_ui_theme_rules() {
        let ok = ThemeConfig {
            accent: Some("#2f6f4e".into()),
            ..Default::default()
        };
        assert_eq!(ok.sanitized_accent(), Some("#2f6f4e"));
        assert!(ok.has_overrides());

        let bad = ThemeConfig {
            accent: Some("red; background:url(x)".into()),
            ..Default::default()
        };
        assert!(bad.sanitized_accent().is_none());
        assert!(!bad.has_overrides());
    }

    #[test]
    fn token_value_length_is_capped() {
        assert!(is_safe_token_value("#abc"));
        assert!(!is_safe_token_value("")); // empty rejected
        assert!(!is_safe_token_value(&"a".repeat(65))); // over the 64-char cap
        assert!(is_safe_token_key("--br-accent-hover"));
        assert!(!is_safe_token_key("--br-")); // empty suffix
        assert!(!is_safe_token_key("accent")); // not a --br- prop
    }
}
