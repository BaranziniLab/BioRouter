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

/// Multi-agent orchestration: sub-agents-as-tools, handoff targets, and
/// declarative workflows. Handoff targets are full `AgentConfig`s (recursion is
/// bounded by the author and the empty-by-default maps).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Orchestration {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub sub_agents: HashMap<String, SubAgentManifest>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub agents: HashMap<String, AgentConfig>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub workflows: HashMap<String, WorkflowManifest>,
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
}

impl Default for SignalDecl {
    fn default() -> Self {
        Self {
            name: String::new(),
            payload: None,
            coalesce_ms: default_coalesce_ms(),
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
