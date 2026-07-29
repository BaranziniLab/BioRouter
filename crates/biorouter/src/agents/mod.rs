pub(crate) mod agent;
// BR-35: the per-reply wall-clock / token / dollar ceiling. Off unless
// configured; the iteration caps (`max_turns`, `max_tool_calls`) bound how many
// steps a reply takes, this bounds how long it runs and what it costs.
pub mod budget;
pub(crate) mod chatrecall_extension;
pub(crate) mod code_execution_extension;
// BR-48: a deterministic done-ness gate for interactive chat — reuses the
// workflow `SuccessCheck` machinery to keep a turn working until its checks
// pass. Config-gated, default OFF.
pub mod done_gate;
pub mod effort;
pub mod execute_commands;
pub mod extension;
pub mod extension_malware_check;
pub mod extension_manager;
pub mod extension_manager_extension;
pub mod final_output_tool;
pub mod goal;
pub mod knowledge_tool;
mod large_response_handler;
pub mod mcp_client;
pub mod mcp_pool;
pub mod mistakes;
pub mod moim;
pub mod platform_tools;
// BR-47: auto post-edit diagnostics — the config gate, write-detection, and
// corrective-context formatting for the edit->check->fix reflection loop.
pub mod post_edit_diagnostics;
// Stage 0 of the tool-call latency work: opt-in per-phase timing behind
// `BIOROUTER_PHASE_TIMING=1`, free when off.
pub mod phase_timing;
pub mod prompt_manager;
mod recurring;
// BR-12: `pub(crate)` so `context_mgmt::run_eager_compaction` can reuse
// `apply_session_metrics` from the background compaction task.
pub(crate) mod reply_parts;
mod resource_refs;
pub mod retry;
mod schedule_tool;
mod session_blob_tool;
// BR-71 decision (c): per-session skill enablement, kept strictly out of the
// machine-wide `skills-config.json`.
pub mod session_skills;
// Pub so the CLI (`biorouter skill …`) reuses the exact same skill discovery
// roots and frontmatter parsing as this backend extension, instead of keeping
// a drifting duplicate (Codex B2 findings 5+6).
pub mod skills_extension;
// BR-50: an optional, config-gated self-critique pass that re-reads an ordinary
// answer for correctness before it is returned, reusing the goal-judge LLM
// primitive. Default OFF (it costs an extra LLM call per turn).
pub mod self_critique;
// BR-32: the `/goal` stall detector, generalized into a periodic no-progress
// check that runs for every long agentic turn, not just goal sessions.
pub mod stall;
pub mod structured_output;
pub mod subagent_execution_tool;
// BR-40: the async half — a background `subagent` call returns a handle the
// parent polls with `subagent_status`, instead of blocking the turn.
pub mod subagent_handle;
pub mod subagent_handler;
pub mod subagent_result;
mod subagent_task_config;
pub mod subagent_tool;
pub(crate) mod todo_extension;
pub mod tool_dispatch_limits;
mod tool_execution;
// BR-51: the structured tool-error taxonomy every failed tool result is reduced
// to, so the model and the loop detectors can tell a retryable blip from a hard
// failure.
pub mod tool_errors;
pub mod turn_abort;
pub mod turn_guard;
pub mod types;
pub mod vault_refs;
// BR-71 §5: the always-confirm hook for cross-session capability changes.
pub mod workspace_extension;
pub mod workspace_inspector;
pub mod workspace_summary;

pub use agent::{
    Agent, AgentConfig, AgentEvent, ConfirmationOutcome, ExtensionLoadResult, PersistedMessage,
};
pub use budget::ReplyBudget;
pub use effort::ReasoningEffort;
pub use execute_commands::COMPACT_TRIGGERS;
pub use extension::ExtensionConfig;
pub use extension_manager::{normalize, ExtensionManager};
pub use prompt_manager::PromptManager;
pub use skills_extension::{count_user_skills, reset_to_builtin_skills};
pub use subagent_handle::{BackgroundSubagent, HandleSnapshot, HandleState};
pub use subagent_result::{SubagentResult, SubagentStatus, SubagentTokens};
pub use subagent_task_config::TaskConfig;
pub use turn_abort::{exit, TurnAbortCode};
pub use types::{FrontendTool, RetryConfig, SessionConfig, SuccessCheck};
pub use vault_refs::VaultRefs;
