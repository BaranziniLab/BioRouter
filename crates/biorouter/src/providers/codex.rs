//! The **Codex** provider: drives the user's own `codex` CLI on the user's own
//! ChatGPT subscription.
//!
//! # Why `codex app-server` and not `codex exec`
//!
//! `codex exec --json` is the obvious choice and it is the wrong one, for a
//! reason that only shows up once tools are involved. `exec` has no channel for
//! answering an approval, so the moment the agent wants to call a tool the call
//! fails with "user cancelled MCP tool call" — verified — and the only ways
//! around it are `--approve-for-me` (which forces a workspace-write sandbox) or
//! `--dangerously-bypass-approvals-and-sandbox`. Both hand the child more
//! authority than Biorouter wants it to have.
//!
//! `codex app-server` speaks JSON-RPC over stdio and routes every approval back
//! to the host as a **server-originated request**, blocking the turn until it is
//! answered. That is precisely the shape Biorouter needs: the decision stays here.
//! It also exposes `account/read`, `account/rateLimits/read` and `model/list`,
//! and its whole protocol can be regenerated with
//! `codex app-server generate-json-schema --out DIR` rather than reverse-engineered.
//!
//! # `thread/start` carries Biorouter's instructions
//!
//! `baseInstructions` replaces Codex's own system prompt with Biorouter's, which
//! is both correct and a large token saving — Codex's default preamble measured
//! ~15k input tokens on a trivial prompt.
//!
//! # What the child may do
//!
//! `sandbox: "read-only"` and no MCP servers, so the child has no way to change
//! anything. Biorouter's own tools reach it over MCP once the bridge lands, and
//! those execute in Biorouter's dispatcher where every existing gate still fires.
//! Until then, an approval request for a command or a file change means the child
//! is trying to do something it was not given, and is refused rather than
//! rubber-stamped.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::{Role, Tool};
use serde_json::{json, Value};

use super::base::{ConfigKey, ModelInfo, Provider, ProviderMetadata, ProviderUsage, Usage};
use super::coding_agent::appserver::{AppServer, Inbound};
use super::coding_agent::{
    self, bridge, discovery, env as agent_env, transcript, CodingAgentKind,
};
use super::errors::ProviderError;
use crate::config::search_path::SearchPaths;
use crate::conversation::message::{Message, MessageContent};
use crate::model::ModelConfig;

const KIND: CodingAgentKind = CodingAgentKind::Codex;

pub const CODEX_DEFAULT_MODEL: &str = "gpt-5.5";
pub const CODEX_DOC_URL: &str = "https://developers.openai.com/codex/cli";

/// A turn's wall-clock ceiling. None of the deleted CLI providers had one, so a
/// wedged child held its session open indefinitely.
const TURN_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Each window must match what `MODEL_CONTEXT_WINDOWS` declares, because
/// `tests/context_windows.rs` compares the two.
fn known_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo::new("gpt-5.5", 1_050_000),
        ModelInfo::new("gpt-5.4", 1_050_000),
        ModelInfo::new("gpt-5.4-mini", 400_000),
        ModelInfo::new("gpt-5.3-codex", 400_000),
    ]
}

#[derive(Debug, serde::Serialize)]
pub struct CodexProvider {
    command: PathBuf,
    model: ModelConfig,
    #[serde(skip)]
    name: String,
}

/// What one turn produced.
#[derive(Default)]
struct TurnOutcome {
    text: Vec<String>,
    usage: Option<Usage>,
    failure: Option<String>,
}

impl CodexProvider {
    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        // Resolution only, no spawning: `GET /config/providers` builds every
        // provider under a 3s timeout to sample tier and affiliation.
        let configured = discovery::configured_command(KIND);
        let command = discovery::resolve_binary(KIND, configured.as_deref()).ok_or_else(|| {
            anyhow::anyhow!(
                "could not find the `{}` command. Install it with:\n    {}\n\nIf it is installed \
                 somewhere Biorouter does not search (nvm, volta, bun, asdf), set {} to its full \
                 path.",
                KIND.default_command(),
                KIND.install_hint(),
                KIND.command_config_key(),
            )
        })?;
        Ok(Self {
            command,
            model,
            name: KIND.provider_id().to_string(),
        })
    }

    fn app_server_command(&self) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(&self.command);
        cmd.arg("app-server");

        // `codex` is an npm shim that execs a sibling native binary and shells out
        // to git and ripgrep on its own account, so the resolved absolute path is
        // not enough — it needs the augmented PATH too.
        if let Ok(path) = SearchPaths::builder().with_npm().path() {
            cmd.env("PATH", path);
        }
        // LAST, after every arg and env — see the ordering note on the function.
        agent_env::configure_subscription_child(&mut cmd);
        cmd
    }

    /// `thread/start` parameters.
    ///
    /// `ephemeral` keeps Codex from writing its own session files: Biorouter owns
    /// the transcript, and a second copy on disk would be governed by none of
    /// Biorouter's controls.
    fn thread_params(system: &str, cwd: &str, model: &str, bridge_url: Option<&str>) -> Value {
        let mut params = json!({
            "cwd": cwd,
            "ephemeral": true,
            "sandbox": "read-only",
            "approvalPolicy": "never",
            "baseInstructions": system,
        });
        if !model.trim().is_empty() {
            params["model"] = json!(model);
        }
        // Biorouter's own tools, when this turn has a bridge. `config` is the same
        // override map `-c` writes, so this is `mcp_servers.<name>.url` — the
        // streamable-HTTP form, which is the only one that needs no second process.
        //
        // ⚠ The URL is the credential. Codex sends no Authorization header on MCP
        // requests (observed), so the capability has to travel in the path, and it
        // must not be logged. `dynamicTools` would remove the HTTP hop entirely and
        // is the eventual replacement — but the installed Codex declares the
        // DynamicToolSpec types without any request that accepts them, so it is not
        // reachable yet.
        if let Some(url) = bridge_url {
            params["config"] = json!({
                "mcp_servers": { "biorouter": { "url": url } }
            });
        }
        params
    }

    /// Answer a server-originated request.
    ///
    /// Anything that would let the child act on the machine is refused. The child
    /// is configured with a read-only sandbox and no tools of its own, so an
    /// approval request here means it is reaching for authority it was not given,
    /// and the honest answer is no. Elicitation is accepted because that is how an
    /// MCP tool call Biorouter itself is serving gets its go-ahead — and those run
    /// in Biorouter's dispatcher, behind Biorouter's gates.
    fn decide(method: &str) -> Value {
        match method {
            "mcpServer/elicitation/request" => json!({ "action": "accept", "content": {} }),
            "item/commandExecution/requestApproval"
            | "item/fileChange/requestApproval"
            | "item/permissions/requestApproval"
            | "applyPatchApproval"
            | "execCommandApproval" => json!({ "decision": "denied" }),
            // An unrecognised request still has to be answered or the turn stalls
            // forever. Refuse rather than guess.
            _ => json!({ "decision": "denied" }),
        }
    }

    /// Fold one notification into the turn's outcome. Returns true when the turn
    /// is over.
    fn absorb(outcome: &mut TurnOutcome, method: &str, params: &Value) -> bool {
        match method {
            "item/completed" => {
                let item = params.get("item").unwrap_or(&Value::Null);
                // The app server uses camelCase item types while `codex exec`
                // uses snake_case; accept both so a version change on either
                // surface does not silently drop the answer.
                let kind = item.get("type").and_then(Value::as_str).unwrap_or_default();
                if matches!(kind, "agentMessage" | "agent_message") {
                    if let Some(text) = item.get("text").and_then(Value::as_str) {
                        if !text.trim().is_empty() {
                            outcome.text.push(text.to_string());
                        }
                    }
                }
                false
            }
            "turn/completed" => {
                outcome.usage = Some(parse_usage(params.get("usage")));
                true
            }
            "turn/failed" => {
                outcome.failure = Some(
                    params
                        .get("error")
                        .and_then(|e| e.get("message").and_then(Value::as_str))
                        .unwrap_or("the Codex turn failed")
                        .to_string(),
                );
                true
            }
            // ⚠ The literal is "error", not "thread.error" — it breaks the dotted
            // convention every sibling notification follows, so a match written
            // from the type names alone misses it.
            "error" => {
                outcome.failure = Some(
                    params
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("the Codex app server reported an error")
                        .to_string(),
                );
                // Advisory errors can precede `turn.started` and are not fatal, so
                // this does not end the turn; `turn/failed` or `turn/completed`
                // does. Recorded in case nothing else explains an empty answer.
                false
            }
            _ => false,
        }
    }

    /// Drive one complete turn: handshake, thread, turn, then pump until done.
    async fn run_turn(
        &self,
        model: &str,
        system: &str,
        prompt: &str,
    ) -> Result<TurnOutcome, ProviderError> {
        let server = AppServer::spawn(self.app_server_command()).await?;
        let result = self.turn_on(&server, model, system, prompt).await;
        // Always reap. A leaked `codex app-server` is a live process holding the
        // user's credential.
        server.shutdown().await;
        result
    }

    async fn turn_on(
        &self,
        server: &AppServer,
        model: &str,
        system: &str,
        prompt: &str,
    ) -> Result<TurnOutcome, ProviderError> {
        server
            .request(
                "initialize",
                json!({
                    // Identify honestly. Some harnesses shape this to look like the
                    // vendor's own first-party client; doing so is exactly what the
                    // vendors' terms prohibit, so Biorouter says who it is.
                    "clientInfo": { "name": "biorouter", "version": env!("CARGO_PKG_VERSION") },
                    "capabilities": { "experimentalApi": true }
                }),
            )
            .await?;
        server.notify("initialized", Value::Null).await?;

        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| ".".to_string());
        let thread = server
            .request(
                "thread/start",
                Self::thread_params(system, &cwd, model, bridge::active_bridge_url().as_deref()),
            )
            .await?;
        let thread_id = thread
            .get("thread")
            .and_then(|t| t.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ProviderError::RequestFailed(
                    "codex app-server did not return a thread id".to_string(),
                )
            })?
            .to_string();

        // `turn/start` is acknowledged immediately and the turn continues as
        // notifications, so the pump has to run alongside it rather than after it.
        let start = server.request(
            "turn/start",
            json!({ "threadId": thread_id, "input": [{ "type": "text", "text": prompt }] }),
        );
        let pump = Self::pump(server);

        let (started, outcome) = tokio::time::timeout(TURN_TIMEOUT, async {
            tokio::join!(start, pump)
        })
        .await
        .map_err(|_| {
            ProviderError::ExecutionError(format!(
                "the Codex turn did not finish within {}s and was stopped",
                TURN_TIMEOUT.as_secs()
            ))
        })?;
        started?;
        outcome
    }

    /// Read notifications until the turn ends, answering server requests as they
    /// arrive.
    async fn pump(server: &AppServer) -> Result<TurnOutcome, ProviderError> {
        let mut outcome = TurnOutcome::default();
        while let Some(message) = server.next_inbound().await {
            match message {
                Inbound::Request { id, method, .. } => {
                    server.respond(&id, Self::decide(&method)).await?;
                }
                Inbound::Notification { method, params } => {
                    if Self::absorb(&mut outcome, &method, &params) {
                        return Ok(outcome);
                    }
                }
            }
        }
        // stdout closed without a terminal frame.
        if outcome.failure.is_none() && outcome.text.is_empty() {
            outcome.failure = Some(format!(
                "the Codex app server exited before finishing the turn{}",
                server.stderr_suffix().await
            ));
        }
        Ok(outcome)
    }
}

/// Map Codex's usage onto Biorouter's four **disjoint** buckets.
///
/// Codex follows OpenAI's convention, where `input_tokens` is the total prompt
/// count and `cached_input_tokens` is the cached *subset* of it. Biorouter's
/// [`Usage`] requires the buckets not to overlap, so the cached part is
/// subtracted out of `input_tokens` here — without that, `billed_total` would
/// double-count every cached token and stop reconciling with a vendor bill.
fn parse_usage(usage: Option<&Value>) -> Usage {
    let Some(u) = usage else {
        return Usage::default();
    };
    let get = |k: &str| u.get(k).and_then(Value::as_i64).map(|v| v as i32);

    let total_input = get("input_tokens");
    let cached = get("cached_input_tokens");
    let cache_write = get("cache_write_input_tokens");
    let output = get("output_tokens");

    let fresh_input = match (total_input, cached) {
        (Some(total), Some(c)) => Some((total - c).max(0)),
        (other, _) => other,
    };

    Usage {
        input_tokens: fresh_input,
        output_tokens: output,
        // Context occupancy for the gauge, which includes the cached prefix.
        total_tokens: match (total_input, output) {
            (None, None) => None,
            _ => Some(total_input.unwrap_or(0) + output.unwrap_or(0)),
        },
        cache_read_input_tokens: cached,
        cache_creation_input_tokens: cache_write,
        ..Default::default()
    }
}

#[async_trait]
impl Provider for CodexProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::with_models(
            KIND.provider_id(),
            KIND.display_name(),
            "Uses the ChatGPT subscription you are already signed in to, through your own \
             installed `codex` command. No API key. Requires the Codex CLI to be installed and \
             signed in.",
            CODEX_DEFAULT_MODEL,
            known_models(),
            CODEX_DOC_URL,
            vec![ConfigKey::new(
                KIND.command_config_key(),
                true,
                false,
                Some(KIND.default_command()),
            )],
        )
        .with_unlisted_models()
        // Public, and not `runs_locally`: the subprocess is local, the inference is
        // OpenAI's. Both are the trait defaults, restated so the decision is
        // visible rather than inherited by omission.
        .with_tier(crate::privacy::ProviderTier::Public)
    }

    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model.clone()
    }

    async fn complete_with_model(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        let prompt = transcript::flatten(messages).ok_or_else(|| {
            ProviderError::RequestFailed("there is no user message for Codex to answer".to_string())
        })?;

        let outcome = self
            .run_turn(&model_config.model_name, system, &prompt)
            .await?;

        if outcome.text.is_empty() {
            let detail = outcome
                .failure
                .unwrap_or_else(|| "Codex returned an empty response".to_string());
            return Err(ProviderError::RequestFailed(detail));
        }

        let message = Message::new(
            Role::Assistant,
            chrono::Utc::now().timestamp(),
            vec![MessageContent::text(outcome.text.join("\n\n"))],
        );

        // Attribute the row to this provider. Left unset, accounting falls back to
        // the model name and `canonical_model_pricing` invents an OpenAI per-token
        // price for a run that billed a subscription.
        let mut usage = ProviderUsage::new(
            model_config.model_name.clone(),
            outcome.usage.unwrap_or_default(),
        );
        usage.provider = Some(KIND.provider_id().to_string());
        Ok((message, usage))
    }

    /// Ask the CLI whether this provider can run at all. Spawns, so never call it
    /// from `from_env`.
    async fn fetch_supported_models(&self) -> Result<Option<Vec<String>>, ProviderError> {
        let availability = discovery::probe(KIND).await;
        if !availability.auth.is_subscription() {
            return Err(coding_agent::unavailable_error(KIND, &availability));
        }
        Ok(Some(known_models().into_iter().map(|m| m.name).collect()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The thread is read-only, ephemeral, and carries Biorouter's instructions.
    /// Each of the four is a decision rather than a default, so each is pinned.
    #[test]
    fn thread_params_are_locked_down() {
        let p = CodexProvider::thread_params("SYSTEM", "/tmp/work", "gpt-5.5", None);
        assert_eq!(p["sandbox"], "read-only", "the child must not be able to write");
        assert_eq!(p["ephemeral"], true, "Codex must not persist its own transcript");
        assert_eq!(p["approvalPolicy"], "never");
        assert_eq!(
            p["baseInstructions"], "SYSTEM",
            "Biorouter's prompt replaces Codex's own preamble"
        );
        assert_eq!(p["cwd"], "/tmp/work");
        assert_eq!(p["model"], "gpt-5.5");
    }

    /// An empty model means "whatever Codex defaults to", which must be expressed
    /// by omitting the key rather than sending an empty string.
    #[test]
    fn an_empty_model_is_omitted_rather_than_sent_blank() {
        let p = CodexProvider::thread_params("S", "/tmp", "   ", None);
        assert!(p.get("model").is_none());
    }

    /// A bridge URL becomes an `mcp_servers` config override, which is how
    /// Biorouter's own tools reach the child. Without a bridge no `config` key is
    /// sent at all, so the child gets no tools rather than an empty server map.
    #[test]
    fn a_bridge_url_becomes_an_mcp_server_override() {
        let with = CodexProvider::thread_params(
            "S",
            "/tmp",
            "gpt-5.5",
            Some("http://127.0.0.1:9/tool_bridge/deadbeef"),
        );
        assert_eq!(
            with["config"]["mcp_servers"]["biorouter"]["url"],
            "http://127.0.0.1:9/tool_bridge/deadbeef"
        );

        let without = CodexProvider::thread_params("S", "/tmp", "gpt-5.5", None);
        assert!(
            without.get("config").is_none(),
            "no bridge must mean no config key, not an empty server map"
        );
    }

    /// Every approval that would let the child act on the machine is refused;
    /// elicitation — how a Biorouter-served MCP tool call is cleared — is accepted.
    #[test]
    fn only_elicitation_is_accepted() {
        assert_eq!(
            CodexProvider::decide("mcpServer/elicitation/request")["action"],
            "accept"
        );
        for refused in [
            "item/commandExecution/requestApproval",
            "item/fileChange/requestApproval",
            "item/permissions/requestApproval",
            "applyPatchApproval",
            "execCommandApproval",
        ] {
            assert_eq!(
                CodexProvider::decide(refused)["decision"],
                "denied",
                "{refused} must be refused: the child has no authority to act"
            );
        }
    }

    /// An unknown request must still receive an answer, or the turn stalls
    /// forever waiting for one.
    #[test]
    fn an_unknown_request_is_still_answered() {
        let answer = CodexProvider::decide("some/future/request");
        assert!(
            answer.is_object() && !answer.as_object().unwrap().is_empty(),
            "an unanswered server request blocks the turn indefinitely"
        );
    }

    /// The real notification sequence, as captured from a live `codex app-server`.
    #[test]
    fn a_captured_turn_sequence_yields_text_and_usage() {
        let mut outcome = TurnOutcome::default();
        let frames = [
            ("item/completed", json!({"item":{"id":"i0","type":"userMessage"}})),
            ("item/completed", json!({"item":{"id":"i1","type":"reasoning"}})),
            (
                "item/completed",
                json!({"item":{"id":"i2","type":"mcpToolCall","server":"biorouter",
                               "tool":"spoke_lookup","status":"completed"}}),
            ),
            (
                "item/completed",
                json!({"item":{"id":"i3","type":"agentMessage","text":"The gene is HLA-DRB1."}}),
            ),
        ];
        for (method, params) in &frames {
            assert!(
                !CodexProvider::absorb(&mut outcome, method, params),
                "{method} must not end the turn"
            );
        }
        assert!(CodexProvider::absorb(
            &mut outcome,
            "turn/completed",
            &json!({"usage":{"input_tokens":15317,"cached_input_tokens":9984,
                             "cache_write_input_tokens":0,"output_tokens":7}}),
        ));

        assert_eq!(outcome.text, vec!["The gene is HLA-DRB1."]);
        let usage = outcome.usage.unwrap();
        // input_tokens is cache-INCLUSIVE upstream, so the fresh count is the
        // difference. Overlapping buckets would double-count the cached prefix.
        assert_eq!(usage.input_tokens, Some(15317 - 9984));
        assert_eq!(usage.cache_read_input_tokens, Some(9984));
        assert_eq!(usage.output_tokens, Some(7));
        assert_eq!(usage.total_tokens, Some(15317 + 7));
    }

    /// `codex exec` spells item types in snake_case where the app server uses
    /// camelCase. Accepting both means a version change on either surface cannot
    /// silently swallow the answer.
    #[test]
    fn both_item_type_spellings_are_accepted() {
        for spelling in ["agentMessage", "agent_message"] {
            let mut outcome = TurnOutcome::default();
            CodexProvider::absorb(
                &mut outcome,
                "item/completed",
                &json!({"item":{"type":spelling,"text":"hello"}}),
            );
            assert_eq!(outcome.text, vec!["hello"], "{spelling} should be read");
        }
    }

    /// The error notification's literal is `error`, breaking the dotted convention
    /// its siblings follow — and it is advisory, so it must not end the turn.
    #[test]
    fn an_advisory_error_is_recorded_without_ending_the_turn() {
        let mut outcome = TurnOutcome::default();
        let ended = CodexProvider::absorb(
            &mut outcome,
            "error",
            &json!({"message":"config warning: feature under development"}),
        );
        assert!(!ended, "an advisory error precedes turn.started and is not fatal");
        assert!(outcome.failure.is_some());

        // …and a later real answer still lands.
        CodexProvider::absorb(
            &mut outcome,
            "item/completed",
            &json!({"item":{"type":"agentMessage","text":"answer"}}),
        );
        assert_eq!(outcome.text, vec!["answer"]);
    }

    #[test]
    fn turn_failed_ends_the_turn_with_its_message() {
        let mut outcome = TurnOutcome::default();
        assert!(CodexProvider::absorb(
            &mut outcome,
            "turn/failed",
            &json!({"error":{"message":"rate limited"}}),
        ));
        assert_eq!(outcome.failure.as_deref(), Some("rate limited"));
    }

    #[test]
    fn absent_usage_is_not_invented() {
        assert_eq!(parse_usage(None).input_tokens, None);
        assert_eq!(parse_usage(None).total_tokens, None);
    }

    #[test]
    fn metadata_is_public_and_keyless_with_one_defaulted_key() {
        let m = CodexProvider::metadata();
        assert_eq!(m.name, "codex");
        assert_eq!(m.display_name, "Codex");
        assert_eq!(m.tier, crate::privacy::ProviderTier::Public);
        assert!(!m.runs_locally, "the subprocess is local but the inference is not");
        assert_eq!(m.config_keys.len(), 1);
        assert_eq!(m.config_keys[0].name, "CODEX_COMMAND");
        assert!(m.config_keys[0].required);
        assert!(!m.config_keys[0].secret);
        assert_eq!(m.config_keys[0].default.as_deref(), Some("codex"));
    }
}
