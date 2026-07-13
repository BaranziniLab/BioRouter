//! User-configurable lifecycle hooks (Claude Code-style).
//!
//! Hooks are shell commands or LLM-judge prompts that run on agent lifecycle
//! events (tool calls, prompts, compaction, session lifecycle). They are
//! configured under `hooks:` in `~/.config/biorouter/config.yaml` and,
//! when enabled via `hooks.allow_project_hooks`, in a project-level
//! `.biorouter/hooks.yaml` inside the session working directory.
//!
//! Execution is failure-open: a crashing, timing-out, or misconfigured hook
//! never blocks the agent. Only explicit decisions block (exit code 2, a
//! `block`/`deny` decision in stdout JSON, or a prompt-hook `ok: false`).

pub mod command_runner;
pub mod config;
pub mod event;
pub mod inspector;
pub mod matcher;
pub mod outcome;
pub mod prompt_runner;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, RwLock};
use tracing::{debug, warn};

pub use config::{HookDefinition, HookMatcherGroup, HooksConfig};
pub use event::{HookEvent, HookPayload};
pub use inspector::HookInspector;
pub use outcome::{HookAggregate, HookDecision, HookOutcome};

use crate::agents::types::SharedProvider;
use crate::managed::ManagedPolicy;
use crate::providers::base::Provider;
use config::CachedProjectHooks;

/// Default timeout for command hooks.
pub const DEFAULT_COMMAND_TIMEOUT_SECS: u64 = 60;
/// Default timeout for prompt (LLM judge) hooks.
pub const DEFAULT_PROMPT_TIMEOUT_SECS: u64 = 30;
/// Maximum consecutive Stop-hook blocks per session before Biorouter
/// overrides the hook and stops anyway.
pub const STOP_HOOK_BLOCK_CAP: u32 = 5;

/// Result of consulting Stop hooks at turn exit.
#[derive(Debug, Clone, PartialEq)]
pub enum StopHookVerdict {
    /// No hook objected — finish the turn.
    Proceed,
    /// A hook blocked the stop; keep working and feed `reason` to the model.
    Blocked { reason: String },
    /// Hooks kept blocking past [`STOP_HOOK_BLOCK_CAP`]; stop anyway.
    CapReached,
}

pub struct HooksManager {
    global: HooksConfig,
    allow_project_hooks: bool,
    project_cache: RwLock<HashMap<PathBuf, CachedProjectHooks>>,
    provider: SharedProvider,
    custom_providers: Mutex<HashMap<(String, String), Arc<dyn Provider>>>,
    sessions_started: Mutex<HashSet<String>>,
    stop_blocks: Mutex<HashMap<String, u32>>,
    /// Hooks registered programmatically at runtime, scoped to one session
    /// (e.g. the `/goal` Stop-hook evaluator). Not persisted; cleared when the
    /// owning feature clears them or the process exits.
    session_hooks: RwLock<HashMap<String, HashMap<HookEvent, Vec<HookDefinition>>>>,
    /// Trusted admin/managed policy (BR-65). Managed hook groups run first and
    /// cannot be disabled; a managed `allow_project_hooks` override wins over
    /// the user/env opt-in. Inert when no managed file is present.
    managed: Arc<ManagedPolicy>,
}

impl HooksManager {
    /// Build from the global Biorouter config, loading the managed policy.
    pub fn new(provider: SharedProvider) -> Self {
        Self::new_with_managed(provider, ManagedPolicy::load())
    }

    /// Build from the global Biorouter config with an already-loaded managed
    /// policy, so the agent can share one instance across hooks + inspectors.
    pub fn new_with_managed(provider: SharedProvider, managed: Arc<ManagedPolicy>) -> Self {
        let global = config::load_global_config();
        let allow_project_hooks = global.allow_project_hooks
            || std::env::var("BIOROUTER_ALLOW_PROJECT_HOOKS")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
        Self::with_config_and_managed(global, allow_project_hooks, provider, managed)
    }

    /// Build with an explicit config (used by tests). No managed policy.
    pub fn with_config(
        global: HooksConfig,
        allow_project_hooks: bool,
        provider: SharedProvider,
    ) -> Self {
        Self::with_config_and_managed(
            global,
            allow_project_hooks,
            provider,
            Arc::new(ManagedPolicy::empty()),
        )
    }

    /// Build with an explicit config and managed policy. A managed
    /// `allow_project_hooks` override, when set, wins over the user/env value.
    pub fn with_config_and_managed(
        global: HooksConfig,
        user_allow_project_hooks: bool,
        provider: SharedProvider,
        managed: Arc<ManagedPolicy>,
    ) -> Self {
        let allow_project_hooks = managed
            .project_hooks_override()
            .unwrap_or(user_allow_project_hooks);
        Self {
            global,
            allow_project_hooks,
            project_cache: RwLock::new(HashMap::new()),
            provider,
            custom_providers: Mutex::new(HashMap::new()),
            sessions_started: Mutex::new(HashSet::new()),
            stop_blocks: Mutex::new(HashMap::new()),
            session_hooks: RwLock::new(HashMap::new()),
            managed,
        }
    }

    /// Replace the runtime hooks for (session, event). Session hooks are
    /// merged into every matching `dispatch` for that session, after the
    /// config-file hooks (most-restrictive decision still wins).
    pub async fn set_session_hooks(
        &self,
        session_id: &str,
        event: HookEvent,
        hooks: Vec<HookDefinition>,
    ) {
        self.session_hooks
            .write()
            .await
            .entry(session_id.to_string())
            .or_default()
            .insert(event, hooks);
    }

    /// Remove the runtime hooks for (session, event).
    pub async fn clear_session_hooks(&self, session_id: &str, event: HookEvent) {
        let mut map = self.session_hooks.write().await;
        if let Some(events) = map.get_mut(session_id) {
            events.remove(&event);
            if events.is_empty() {
                map.remove(session_id);
            }
        }
    }

    /// Whether any runtime hook is registered for (session, event).
    pub async fn has_session_hooks(&self, session_id: &str, event: HookEvent) -> bool {
        self.session_hooks
            .read()
            .await
            .get(session_id)
            .and_then(|events| events.get(&event))
            .is_some_and(|hooks| !hooks.is_empty())
    }

    async fn session_definitions(&self, session_id: &str, event: HookEvent) -> Vec<HookDefinition> {
        self.session_hooks
            .read()
            .await
            .get(session_id)
            .and_then(|events| events.get(&event))
            .cloned()
            .unwrap_or_default()
    }

    /// Consecutive Stop-hook blocks recorded for a session (resets when tools
    /// run or a stop proceeds). At [`STOP_HOOK_BLOCK_CAP`] the cap was hit.
    pub async fn stop_block_count(&self, session_id: &str) -> u32 {
        self.stop_blocks
            .lock()
            .await
            .get(session_id)
            .copied()
            .unwrap_or(0)
    }

    /// Resolved config for a working dir, in precedence order: managed groups
    /// (admin tier, always run, cannot be disabled), then global, then project.
    /// Merge semantics are unchanged (most-restrictive decision wins), so a
    /// managed `Stop`/`PreToolUse` block cannot be undone by a user hook.
    async fn resolved_groups(&self, event: HookEvent, working_dir: &Path) -> Vec<HookMatcherGroup> {
        let mut groups = self
            .managed
            .hooks()
            .events
            .get(&event)
            .cloned()
            .unwrap_or_default();
        groups.extend(self.global.events.get(&event).cloned().unwrap_or_default());
        if self.allow_project_hooks {
            let project = self.project_config(working_dir).await;
            if let Some(project_groups) = project.events.get(&event) {
                groups.extend(project_groups.clone());
            }
        }
        groups
    }

    async fn project_config(&self, working_dir: &Path) -> HooksConfig {
        let current_mtime = config::project_hooks_mtime(working_dir);
        {
            let cache = self.project_cache.read().await;
            if let Some(cached) = cache.get(working_dir) {
                if cached.mtime == current_mtime {
                    return cached.config.clone();
                }
            }
        }
        let fresh = config::read_project_hooks(working_dir);
        let result = fresh.config.clone();
        self.project_cache
            .write()
            .await
            .insert(working_dir.to_path_buf(), fresh);
        result
    }

    /// Cheap check whether any hook could run for this event + matcher key.
    pub async fn has_hooks(
        &self,
        event: HookEvent,
        matcher_key: Option<&str>,
        working_dir: &Path,
    ) -> bool {
        self.resolved_groups(event, working_dir)
            .await
            .iter()
            .any(|group| {
                !group.hooks.is_empty()
                    && match matcher_key {
                        Some(key) => matcher::matcher_matches(group.matcher.as_deref(), key),
                        None => true,
                    }
            })
    }

    /// Run all hooks matching (event, matcher_key) concurrently and merge
    /// their outcomes. Never returns an error: hook failures are recorded in
    /// the aggregate and treated as non-blocking.
    pub async fn dispatch(
        &self,
        event: HookEvent,
        matcher_key: Option<&str>,
        payload: &HookPayload,
        working_dir: &Path,
    ) -> HookAggregate {
        let groups = self.resolved_groups(event, working_dir).await;
        let mut definitions: Vec<HookDefinition> = groups
            .iter()
            .filter(|group| match matcher_key {
                Some(key) => matcher::matcher_matches(group.matcher.as_deref(), key),
                None => true,
            })
            .flat_map(|group| group.hooks.iter().cloned())
            .collect();

        // Runtime session-scoped hooks always match (no matcher).
        definitions.extend(self.session_definitions(&payload.session_id, event).await);

        if definitions.is_empty() {
            return HookAggregate::default();
        }

        debug!(
            "hooks: dispatching {} hook(s) for {} (matcher key: {:?})",
            definitions.len(),
            event,
            matcher_key
        );

        let payload_json = payload.to_json();
        let futures = definitions
            .into_iter()
            .map(|definition| self.run_one(definition, event, &payload_json, payload, working_dir));
        let outcomes = futures::future::join_all(futures).await;

        let aggregate = outcome::merge_outcomes(outcomes);
        for error in &aggregate.errors {
            warn!("hooks: non-blocking hook failure on {}: {}", event, error);
        }
        aggregate
    }

    /// Fire-and-forget dispatch for observe-only events; never blocks the
    /// agent loop.
    pub fn fire(
        self: &Arc<Self>,
        event: HookEvent,
        matcher_key: Option<String>,
        payload: HookPayload,
        working_dir: PathBuf,
    ) {
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            manager
                .dispatch(event, matcher_key.as_deref(), &payload, &working_dir)
                .await;
        });
    }

    async fn run_one(
        &self,
        definition: HookDefinition,
        event: HookEvent,
        payload_json: &str,
        payload: &HookPayload,
        working_dir: &Path,
    ) -> HookOutcome {
        match definition {
            HookDefinition::Command { command, timeout } => {
                let timeout =
                    Duration::from_secs(timeout.unwrap_or(DEFAULT_COMMAND_TIMEOUT_SECS).max(1));
                let envs = vec![
                    (
                        "BIOROUTER_HOOK_EVENT".to_string(),
                        event.as_str().to_string(),
                    ),
                    (
                        "BIOROUTER_SESSION_ID".to_string(),
                        payload.session_id.clone(),
                    ),
                    (
                        "BIOROUTER_PROJECT_DIR".to_string(),
                        working_dir.to_string_lossy().to_string(),
                    ),
                ];
                match command_runner::run_command_hook(
                    &command,
                    payload_json,
                    working_dir,
                    &envs,
                    timeout,
                )
                .await
                {
                    Ok(result) => outcome::interpret_command_result(
                        event,
                        result.exit_code,
                        &result.stdout,
                        &result.stderr,
                    ),
                    Err(e) => HookOutcome {
                        error: Some(format!("'{command}': {e}")),
                        ..Default::default()
                    },
                }
            }
            HookDefinition::Prompt {
                prompt,
                model,
                provider,
                timeout,
            } => {
                let timeout =
                    Duration::from_secs(timeout.unwrap_or(DEFAULT_PROMPT_TIMEOUT_SECS).max(1));
                let resolved = self.resolve_prompt_provider(provider, model).await;
                match resolved {
                    Some(provider) => {
                        prompt_runner::run_prompt_hook(
                            provider,
                            event,
                            &prompt,
                            payload_json,
                            timeout,
                        )
                        .await
                    }
                    None => HookOutcome {
                        error: Some("prompt hook: no provider available".to_string()),
                        ..Default::default()
                    },
                }
            }
        }
    }

    /// Provider for a prompt hook: an explicit provider+model pair builds
    /// (and caches) a dedicated provider; otherwise the agent's provider is
    /// used (its fast model when configured).
    async fn resolve_prompt_provider(
        &self,
        provider_name: Option<String>,
        model: Option<String>,
    ) -> Option<Arc<dyn Provider>> {
        if let (Some(name), Some(model)) = (provider_name, model) {
            let key = (name.clone(), model.clone());
            {
                let cache = self.custom_providers.lock().await;
                if let Some(provider) = cache.get(&key) {
                    return Some(Arc::clone(provider));
                }
            }
            let model_config = match crate::model::ModelConfig::new(&model) {
                Ok(config) => config,
                Err(e) => {
                    warn!("hooks: invalid prompt hook model '{}': {}", model, e);
                    return None;
                }
            };
            match crate::providers::create(&name, model_config).await {
                Ok(provider) => {
                    self.custom_providers
                        .lock()
                        .await
                        .insert(key, Arc::clone(&provider));
                    Some(provider)
                }
                Err(e) => {
                    warn!(
                        "hooks: failed to create prompt hook provider '{}': {}",
                        name, e
                    );
                    None
                }
            }
        } else {
            self.provider.lock().await.clone()
        }
    }

    // ---- Typed wrappers used by the agent loop ----

    pub async fn pre_tool_use(
        &self,
        session_id: &str,
        working_dir: &Path,
        tool_name: &str,
        tool_input: &serde_json::Value,
    ) -> HookAggregate {
        let mut payload = HookPayload::new(
            HookEvent::PreToolUse,
            session_id,
            working_dir.to_string_lossy(),
        );
        payload.tool_name = Some(tool_name.to_string());
        payload.tool_input = Some(tool_input.clone());
        self.dispatch(
            HookEvent::PreToolUse,
            Some(tool_name),
            &payload,
            working_dir,
        )
        .await
    }

    pub async fn permission_request(
        &self,
        session_id: &str,
        working_dir: &Path,
        tool_name: &str,
        tool_input: &serde_json::Value,
    ) -> HookAggregate {
        let mut payload = HookPayload::new(
            HookEvent::PermissionRequest,
            session_id,
            working_dir.to_string_lossy(),
        );
        payload.tool_name = Some(tool_name.to_string());
        payload.tool_input = Some(tool_input.clone());
        self.dispatch(
            HookEvent::PermissionRequest,
            Some(tool_name),
            &payload,
            working_dir,
        )
        .await
    }

    pub async fn user_prompt_submit(
        &self,
        session_id: &str,
        working_dir: &Path,
        prompt: &str,
    ) -> HookAggregate {
        let mut payload = HookPayload::new(
            HookEvent::UserPromptSubmit,
            session_id,
            working_dir.to_string_lossy(),
        );
        payload.prompt = Some(prompt.to_string());
        self.dispatch(HookEvent::UserPromptSubmit, None, &payload, working_dir)
            .await
    }

    /// Consult Stop hooks at turn exit, enforcing the consecutive-block cap.
    /// `transcript_tail` (recent conversation, role-prefixed and truncated) is
    /// passed through to hooks so judges can evaluate what the agent did.
    pub async fn stop(
        &self,
        session_id: &str,
        working_dir: &Path,
        transcript_tail: Option<String>,
    ) -> StopHookVerdict {
        if !self.has_hooks(HookEvent::Stop, None, working_dir).await
            && !self.has_session_hooks(session_id, HookEvent::Stop).await
        {
            return StopHookVerdict::Proceed;
        }
        let blocks_so_far = {
            let blocks = self.stop_blocks.lock().await;
            blocks.get(session_id).copied().unwrap_or(0)
        };
        if blocks_so_far >= STOP_HOOK_BLOCK_CAP {
            warn!(
                "hooks: Stop hook block cap ({}) reached for session {}; stopping anyway",
                STOP_HOOK_BLOCK_CAP, session_id
            );
            return StopHookVerdict::CapReached;
        }
        let mut payload =
            HookPayload::new(HookEvent::Stop, session_id, working_dir.to_string_lossy());
        payload.stop_hook_active = Some(blocks_so_far > 0);
        payload.transcript_tail = transcript_tail;
        let aggregate = self
            .dispatch(HookEvent::Stop, None, &payload, working_dir)
            .await;
        match aggregate.decision {
            Some(HookDecision::Deny { reason }) => {
                let mut blocks = self.stop_blocks.lock().await;
                *blocks.entry(session_id.to_string()).or_insert(0) += 1;
                StopHookVerdict::Blocked { reason }
            }
            _ => {
                self.reset_stop_blocks(session_id).await;
                StopHookVerdict::Proceed
            }
        }
    }

    /// Reset the consecutive Stop-block counter (new turn or tools ran).
    pub async fn reset_stop_blocks(&self, session_id: &str) {
        self.stop_blocks.lock().await.remove(session_id);
    }

    /// Fire SessionStart hooks at most once per session per process.
    /// Returns None when already fired.
    pub async fn session_start_once(
        &self,
        session_id: &str,
        working_dir: &Path,
        source: &str,
    ) -> Option<HookAggregate> {
        {
            let mut started = self.sessions_started.lock().await;
            if !started.insert(session_id.to_string()) {
                return None;
            }
        }
        if !self
            .has_hooks(HookEvent::SessionStart, Some(source), working_dir)
            .await
        {
            return None;
        }
        let mut payload = HookPayload::new(
            HookEvent::SessionStart,
            session_id,
            working_dir.to_string_lossy(),
        );
        payload.source = Some(source.to_string());
        Some(
            self.dispatch(HookEvent::SessionStart, Some(source), &payload, working_dir)
                .await,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager_with_yaml(yaml: &str) -> Arc<HooksManager> {
        let config: HooksConfig = serde_yaml::from_str(yaml).unwrap();
        Arc::new(HooksManager::with_config(
            config,
            false,
            Arc::new(Mutex::new(None)),
        ))
    }

    #[tokio::test]
    async fn dispatch_runs_matching_hooks_only() {
        let manager = manager_with_yaml(
            r#"
PreToolUse:
  - matcher: "developer__shell"
    hooks:
      - type: command
        command: "echo '{\"hookSpecificOutput\":{\"permissionDecision\":\"deny\",\"permissionDecisionReason\":\"shell blocked\"}}'"
  - matcher: "other_tool"
    hooks:
      - type: command
        command: "exit 2"
"#,
        );
        let aggregate = manager
            .pre_tool_use(
                "s1",
                Path::new("/tmp"),
                "developer__shell",
                &serde_json::json!({}),
            )
            .await;
        assert!(aggregate.is_denied());
        assert_eq!(aggregate.deny_reason(), Some("shell blocked"));

        let aggregate = manager
            .pre_tool_use(
                "s1",
                Path::new("/tmp"),
                "unmatched_tool",
                &serde_json::json!({}),
            )
            .await;
        assert!(aggregate.decision.is_none());
    }

    #[tokio::test]
    async fn multiple_hooks_merge_most_restrictive() {
        let manager = manager_with_yaml(
            r#"
PreToolUse:
  - hooks:
      - type: command
        command: "echo '{\"hookSpecificOutput\":{\"permissionDecision\":\"allow\"}}'"
      - type: command
        command: "echo blocked >&2; exit 2"
"#,
        );
        let aggregate = manager
            .pre_tool_use("s1", Path::new("/tmp"), "any", &serde_json::json!({}))
            .await;
        assert!(aggregate.is_denied());
        assert_eq!(aggregate.deny_reason(), Some("blocked"));
    }

    #[tokio::test]
    async fn failing_hook_is_failure_open() {
        let manager = manager_with_yaml(
            r#"
PreToolUse:
  - hooks:
      - type: command
        command: "exit 1"
      - type: command
        command: "definitely-not-a-real-binary-biorouter"
"#,
        );
        let aggregate = manager
            .pre_tool_use("s1", Path::new("/tmp"), "any", &serde_json::json!({}))
            .await;
        assert!(aggregate.decision.is_none());
        assert!(!aggregate.errors.is_empty());
    }

    #[tokio::test]
    async fn stop_cap_enforced() {
        let manager = manager_with_yaml(
            r#"
Stop:
  - hooks:
      - type: command
        command: "echo '{\"decision\":\"block\",\"reason\":\"keep going\"}'"
"#,
        );
        for _ in 0..STOP_HOOK_BLOCK_CAP {
            let verdict = manager.stop("s1", Path::new("/tmp"), None).await;
            assert_eq!(
                verdict,
                StopHookVerdict::Blocked {
                    reason: "keep going".to_string()
                }
            );
        }
        assert_eq!(
            manager.stop("s1", Path::new("/tmp"), None).await,
            StopHookVerdict::CapReached
        );
        manager.reset_stop_blocks("s1").await;
        assert!(matches!(
            manager.stop("s1", Path::new("/tmp"), None).await,
            StopHookVerdict::Blocked { .. }
        ));
    }

    #[tokio::test]
    async fn stop_without_hooks_proceeds() {
        let manager = manager_with_yaml("{}");
        assert_eq!(
            manager.stop("s1", Path::new("/tmp"), None).await,
            StopHookVerdict::Proceed
        );
    }

    #[tokio::test]
    async fn stop_hook_active_flag_set_on_second_block() {
        // Hook echoes back whether stop_hook_active was true by blocking
        // only when it is false — second call should then proceed.
        let manager = manager_with_yaml(
            r#"
Stop:
  - hooks:
      - type: command
        command: "if grep -q '\"stop_hook_active\":false' -; then echo '{\"decision\":\"block\",\"reason\":\"first\"}'; fi"
"#,
        );
        assert!(matches!(
            manager.stop("s1", Path::new("/tmp"), None).await,
            StopHookVerdict::Blocked { .. }
        ));
        assert_eq!(
            manager.stop("s1", Path::new("/tmp"), None).await,
            StopHookVerdict::Proceed
        );
    }

    #[tokio::test]
    async fn session_start_fires_once() {
        let manager = manager_with_yaml(
            r#"
SessionStart:
  - hooks:
      - type: command
        command: "echo 'remember the lab protocol'"
"#,
        );
        let first = manager
            .session_start_once("s1", Path::new("/tmp"), "startup")
            .await;
        assert_eq!(
            first.unwrap().joined_context().as_deref(),
            Some("remember the lab protocol")
        );
        assert!(manager
            .session_start_once("s1", Path::new("/tmp"), "startup")
            .await
            .is_none());
    }

    #[tokio::test]
    async fn session_hooks_merge_into_dispatch_for_their_session_only() {
        let manager = manager_with_yaml("{}");
        manager
            .set_session_hooks(
                "s1",
                HookEvent::PreToolUse,
                vec![HookDefinition::Command {
                    command: "echo blocked >&2; exit 2".to_string(),
                    timeout: None,
                }],
            )
            .await;

        let aggregate = manager
            .pre_tool_use("s1", Path::new("/tmp"), "any", &serde_json::json!({}))
            .await;
        assert!(aggregate.is_denied());

        let aggregate = manager
            .pre_tool_use("s2", Path::new("/tmp"), "any", &serde_json::json!({}))
            .await;
        assert!(aggregate.decision.is_none());

        manager
            .clear_session_hooks("s1", HookEvent::PreToolUse)
            .await;
        assert!(!manager.has_session_hooks("s1", HookEvent::PreToolUse).await);
        let aggregate = manager
            .pre_tool_use("s1", Path::new("/tmp"), "any", &serde_json::json!({}))
            .await;
        assert!(aggregate.decision.is_none());
    }

    #[tokio::test]
    async fn stop_consults_session_hooks() {
        let manager = manager_with_yaml("{}");
        // No config hooks and no session hooks: proceed.
        assert_eq!(
            manager.stop("s1", Path::new("/tmp"), None).await,
            StopHookVerdict::Proceed
        );

        manager
            .set_session_hooks(
                "s1",
                HookEvent::Stop,
                vec![HookDefinition::Command {
                    command: "echo '{\"decision\":\"block\",\"reason\":\"goal not met\"}'"
                        .to_string(),
                    timeout: None,
                }],
            )
            .await;
        assert_eq!(
            manager.stop("s1", Path::new("/tmp"), None).await,
            StopHookVerdict::Blocked {
                reason: "goal not met".to_string()
            }
        );
        // Other sessions are unaffected.
        assert_eq!(
            manager.stop("other", Path::new("/tmp"), None).await,
            StopHookVerdict::Proceed
        );

        manager.clear_session_hooks("s1", HookEvent::Stop).await;
        manager.reset_stop_blocks("s1").await;
        assert_eq!(
            manager.stop("s1", Path::new("/tmp"), None).await,
            StopHookVerdict::Proceed
        );
    }

    #[tokio::test]
    async fn stop_passes_transcript_tail_to_hooks() {
        // The hook blocks only when the transcript tail contains the marker,
        // proving the tail reaches hook stdin.
        let manager = manager_with_yaml(
            r#"
Stop:
  - hooks:
      - type: command
        command: "if grep -q 'TAIL_MARKER' -; then echo '{\"decision\":\"block\",\"reason\":\"saw tail\"}'; fi"
"#,
        );
        assert_eq!(
            manager
                .stop("s1", Path::new("/tmp"), Some("TAIL_MARKER".to_string()))
                .await,
            StopHookVerdict::Blocked {
                reason: "saw tail".to_string()
            }
        );
        manager.reset_stop_blocks("s1").await;
        assert_eq!(
            manager.stop("s1", Path::new("/tmp"), None).await,
            StopHookVerdict::Proceed
        );
    }

    #[tokio::test]
    async fn project_hooks_require_opt_in() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".biorouter")).unwrap();
        std::fs::write(
            dir.path().join(".biorouter/hooks.yaml"),
            "hooks:\n  PreToolUse:\n    - hooks: [{ type: command, command: \"exit 2\" }]\n",
        )
        .unwrap();

        let disabled = Arc::new(HooksManager::with_config(
            HooksConfig::default(),
            false,
            Arc::new(Mutex::new(None)),
        ));
        let aggregate = disabled
            .pre_tool_use("s1", dir.path(), "any", &serde_json::json!({}))
            .await;
        assert!(aggregate.decision.is_none());

        let enabled = Arc::new(HooksManager::with_config(
            HooksConfig::default(),
            true,
            Arc::new(Mutex::new(None)),
        ));
        let aggregate = enabled
            .pre_tool_use("s1", dir.path(), "any", &serde_json::json!({}))
            .await;
        assert!(aggregate.is_denied());
    }

    #[tokio::test]
    async fn project_hooks_reload_on_mtime_change() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".biorouter")).unwrap();
        let path = dir.path().join(".biorouter/hooks.yaml");
        std::fs::write(&path, "hooks: {}\n").unwrap();

        let manager = Arc::new(HooksManager::with_config(
            HooksConfig::default(),
            true,
            Arc::new(Mutex::new(None)),
        ));
        let aggregate = manager
            .pre_tool_use("s1", dir.path(), "any", &serde_json::json!({}))
            .await;
        assert!(aggregate.decision.is_none());

        std::fs::write(
            &path,
            "hooks:\n  PreToolUse:\n    - hooks: [{ type: command, command: \"exit 2\" }]\n",
        )
        .unwrap();
        // Force a different mtime in case the filesystem clock is coarse.
        let new_time = std::time::SystemTime::now() + Duration::from_secs(2);
        let _ = filetime_set(&path, new_time);

        let aggregate = manager
            .pre_tool_use("s1", dir.path(), "any", &serde_json::json!({}))
            .await;
        assert!(aggregate.is_denied());
    }

    fn filetime_set(path: &Path, time: std::time::SystemTime) -> std::io::Result<()> {
        let file = std::fs::OpenOptions::new().append(true).open(path)?;
        file.set_modified(time)
    }

    #[tokio::test]
    async fn prompt_hook_without_provider_fails_open() {
        let manager = manager_with_yaml(
            r#"
PreToolUse:
  - hooks:
      - type: prompt
        prompt: "Never allow anything."
"#,
        );
        let aggregate = manager
            .pre_tool_use("s1", Path::new("/tmp"), "any", &serde_json::json!({}))
            .await;
        assert!(aggregate.decision.is_none());
        assert!(!aggregate.errors.is_empty());
    }

    // ---- BR-65: managed/enterprise hook tier ----

    fn managed_from_yaml(yaml: &str) -> Arc<crate::managed::ManagedPolicy> {
        let file: crate::managed::ManagedPolicyFile =
            serde_yaml::from_str(yaml).expect("managed yaml parses");
        Arc::new(crate::managed::ManagedPolicy::from_file(file))
    }

    /// A managed Stop hook that blocks cannot be cleared by a user Stop hook: the
    /// merge takes the most-restrictive decision, so a managed block wins.
    #[tokio::test]
    async fn managed_stop_hook_block_survives_user_hook() {
        let global: HooksConfig =
            serde_yaml::from_str("Stop:\n  - hooks: [{ type: command, command: \"echo ok\" }]\n")
                .unwrap();
        let managed = managed_from_yaml(
            "hooks:\n  Stop:\n    - hooks:\n        - type: command\n          command: \"echo '{\\\"decision\\\":\\\"block\\\",\\\"reason\\\":\\\"managed policy: finish the audit\\\"}'\"\n",
        );
        let manager = Arc::new(HooksManager::with_config_and_managed(
            global,
            false,
            Arc::new(Mutex::new(None)),
            managed,
        ));
        assert_eq!(
            manager.stop("s1", Path::new("/tmp"), None).await,
            StopHookVerdict::Blocked {
                reason: "managed policy: finish the audit".to_string()
            }
        );
    }

    /// Managed hook groups resolve before global groups (admin tier first).
    #[tokio::test]
    async fn managed_hook_groups_resolve_before_global() {
        let global: HooksConfig = serde_yaml::from_str(
            "PreToolUse:\n  - hooks: [{ type: command, command: \"echo global\" }]\n",
        )
        .unwrap();
        let managed = managed_from_yaml(
            "hooks:\n  PreToolUse:\n    - hooks: [{ type: command, command: \"echo managed\" }]\n",
        );
        let manager = Arc::new(HooksManager::with_config_and_managed(
            global,
            false,
            Arc::new(Mutex::new(None)),
            managed,
        ));
        let groups = manager
            .resolved_groups(HookEvent::PreToolUse, Path::new("/tmp"))
            .await;
        assert_eq!(groups.len(), 2);
        assert!(matches!(
            &groups[0].hooks[0],
            HookDefinition::Command { command, .. } if command == "echo managed"
        ));
        assert!(matches!(
            &groups[1].hooks[0],
            HookDefinition::Command { command, .. } if command == "echo global"
        ));
    }

    /// A managed `allow_project_hooks: false` override suppresses a present
    /// project hooks file even when the user opted in with `true`.
    #[tokio::test]
    async fn managed_forbids_project_hooks_over_user_opt_in() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".biorouter")).unwrap();
        std::fs::write(
            dir.path().join(".biorouter/hooks.yaml"),
            "hooks:\n  PreToolUse:\n    - hooks: [{ type: command, command: \"exit 2\" }]\n",
        )
        .unwrap();

        // User opts in (true), but a managed override forbids project hooks.
        let managed = managed_from_yaml("allow_project_hooks: false\n");
        let forbidden = Arc::new(HooksManager::with_config_and_managed(
            HooksConfig::default(),
            true,
            Arc::new(Mutex::new(None)),
            managed,
        ));
        let aggregate = forbidden
            .pre_tool_use("s1", dir.path(), "any", &serde_json::json!({}))
            .await;
        assert!(
            aggregate.decision.is_none(),
            "managed override should suppress project hooks"
        );

        // Sanity: without the managed override, the same user opt-in runs them.
        let allowed = Arc::new(HooksManager::with_config_and_managed(
            HooksConfig::default(),
            true,
            Arc::new(Mutex::new(None)),
            Arc::new(crate::managed::ManagedPolicy::empty()),
        ));
        let aggregate = allowed
            .pre_tool_use("s1", dir.path(), "any", &serde_json::json!({}))
            .await;
        assert!(aggregate.is_denied());
    }

    /// A managed `allow_project_hooks: true` override forces project hooks on
    /// even when the user did not opt in.
    #[tokio::test]
    async fn managed_forces_project_hooks_over_user_opt_out() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".biorouter")).unwrap();
        std::fs::write(
            dir.path().join(".biorouter/hooks.yaml"),
            "hooks:\n  PreToolUse:\n    - hooks: [{ type: command, command: \"exit 2\" }]\n",
        )
        .unwrap();

        let managed = managed_from_yaml("allow_project_hooks: true\n");
        let manager = Arc::new(HooksManager::with_config_and_managed(
            HooksConfig::default(),
            false, // user did NOT opt in
            Arc::new(Mutex::new(None)),
            managed,
        ));
        let aggregate = manager
            .pre_tool_use("s1", dir.path(), "any", &serde_json::json!({}))
            .await;
        assert!(
            aggregate.is_denied(),
            "managed true override should force project hooks on"
        );
    }
}
