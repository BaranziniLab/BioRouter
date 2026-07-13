mod agent;
// BR-35: the per-reply wall-clock / token / dollar ceiling. Off unless
// configured; the iteration caps (`max_turns`, `max_tool_calls`) bound how many
// steps a reply takes, this bounds how long it runs and what it costs.
pub mod budget;
pub(crate) mod chatrecall_extension;
pub(crate) mod code_execution_extension;
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
pub mod mistakes;
pub mod moim;
pub mod platform_tools;
pub mod prompt_manager;
mod recurring;
// BR-12: `pub(crate)` so `context_mgmt::run_eager_compaction` can reuse
// `apply_session_metrics` from the background compaction task.
pub(crate) mod reply_parts;
mod resource_refs;
pub mod retry;
mod schedule_tool;
mod session_blob_tool;
pub(crate) mod skills_extension;
// BR-32: the `/goal` stall detector, generalized into a periodic no-progress
// check that runs for every long agentic turn, not just goal sessions.
pub mod stall;
pub mod structured_output;
pub mod subagent_execution_tool;
pub mod subagent_handler;
pub mod subagent_result;
mod subagent_task_config;
pub mod subagent_tool;
pub(crate) mod todo_extension;
mod tool_execution;
// BR-51: the structured tool-error taxonomy every failed tool result is reduced
// to, so the model and the loop detectors can tell a retryable blip from a hard
// failure.
pub mod tool_errors;
pub mod types;
pub mod vault_refs;
pub mod workspace_summary;

pub use agent::{Agent, AgentConfig, AgentEvent, ConfirmationOutcome, ExtensionLoadResult};
pub use budget::ReplyBudget;
pub use effort::ReasoningEffort;
pub use execute_commands::COMPACT_TRIGGERS;
pub use extension::ExtensionConfig;
pub use extension_manager::{normalize, ExtensionManager};
pub use prompt_manager::PromptManager;
pub use subagent_result::{SubagentResult, SubagentStatus, SubagentTokens};
pub use subagent_task_config::TaskConfig;
pub use types::{FrontendTool, RetryConfig, SessionConfig, SuccessCheck};
pub use vault_refs::VaultRefs;
