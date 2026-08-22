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

use super::base::{
    ConfigKey, MessageStream, ModelInfo, Provider, ProviderMetadata, ProviderStreamItem,
    ProviderUsage, Usage,
};
use super::coding_agent::appserver::{AppServer, Inbound};
use super::coding_agent::{
    self, bridge, codex_stream, discovery, env as agent_env, transcript, CodingAgentKind,
};
use super::errors::ProviderError;
use crate::agents::effort::ReasoningEffort;
use crate::config::search_path::SearchPaths;
use crate::conversation::message::{Message, MessageContent};
use crate::model::ModelConfig;

const KIND: CodingAgentKind = CodingAgentKind::Codex;

pub const CODEX_DEFAULT_MODEL: &str = "gpt-5.5";
pub const CODEX_DOC_URL: &str = "https://developers.openai.com/codex/cli";

/// A turn's wall-clock ceiling. None of the deleted CLI providers had one, so a
/// wedged child held its session open indefinitely.
const TURN_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// How long the turn will wait for the app server to say which account it bills.
///
/// `AppServer::request` has no timeout of its own — it awaits its oneshot until
/// the child's stdout closes — which is right for `thread/start`, whose answer
/// legitimately takes as long as the model does, and wrong for this. The check
/// is documented as fail-open ("a failed request is not a failed check"), and
/// that only holds for an app server that *answers* an unknown method with a
/// JSON-RPC error. One that silently ignores it never resolves, and the turn
/// hangs on its very first round trip with no error, no output and nothing to
/// time it out — the same shape as the Versa/Bedrock freeze already in this
/// repo's history, which is why the mitigation is here rather than filed.
///
/// Ten seconds because the answer is local: the app server reads its own auth
/// state and replies. Anything approaching this is already a broken install, and
/// exceeding it lands in the same fail-open branch a rejected method does.
const ACCOUNT_READ_TIMEOUT: Duration = Duration::from_secs(10);

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

/// The sentence to append to a failed turn when the model name is the likely
/// cause.
///
/// Codex's app server reports an unknown model as an `error` notification with
/// **no message and no category**, so the turn surfaces as "the Codex app
/// server reported an error" plus the generic invitation to retry — advice that
/// can never come true, for a request that will fail identically forever.
/// Measured: `gpt-5.5-codex` (a plausible-looking name that does not exist)
/// produced exactly that, and nothing in it named the model.
///
/// Deliberately a *hint on the failure path* rather than a check before the
/// call. An unlisted name is usually a typo and occasionally a model OpenAI
/// shipped after this list was written, and refusing the second to catch the
/// first would make Biorouter the reason a working model cannot be used. So an
/// unlisted model is still sent; it just stops failing anonymously.
fn unknown_model_hint(model: &str) -> String {
    let known = known_models();
    if known.iter().any(|m| m.name == model) {
        return String::new();
    }
    let names: Vec<&str> = known.iter().map(|m| m.name.as_str()).collect();
    format!(
        " — and `{model}` is not one of the models this build knows Codex to \
         offer ({}). If that name is a typo, no retry will fix it",
        names.join(", ")
    )
}

#[derive(Debug, serde::Serialize)]
pub struct CodexProvider {
    command: PathBuf,
    model: ModelConfig,
    #[serde(skip)]
    name: String,
}

/// What one turn produced.
#[derive(Default, Debug)]
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
        // Codex's own built-in tools cannot be switched off the way Claude Code's
        // can (`tools.exec=false` was tried and does not remove `exec`; only the
        // sandbox constrains it). Its web tool CAN be removed, and is: a child
        // reaching the network on its own account is egress Biorouter never saw,
        // and Biorouter has its own web tools behind its own gates.
        let mut config = json!({ "tools": { "web_search": false } });
        if let Some(url) = bridge_url {
            // ⚠ KNOWN GAP, and it is not symmetric with Claude Code.
            //
            // This ADDS `biorouter` to the child's MCP servers; it does not
            // replace the set. Codex has no `--strict-mcp-config` equivalent, and
            // this `config` object is merged with the user's own
            // `~/.codex/config.toml` rather than overriding it — so any MCP server
            // declared there is also loaded into the child, and its tools execute
            // outside Biorouter's inspectors, permission mode, `.biorouterignore`
            // and vault. Measured against codex 0.147.0 with a canary server in a
            // scratch `CODEX_HOME`.
            //
            // Claude Code closes exactly this with `--strict-mcp-config`, so do
            // NOT read the two providers as equivalently isolated. The intended
            // fix is to point the child at a scratch `CODEX_HOME` carrying only
            // the auth file, which needs a live signed-in Codex to validate
            // (a scratch home that loses the credential breaks the provider
            // outright). Until then this is disclosed in
            // `docs/providers/coding-agents/child-agent-isolation.md` rather than
            // papered over.
            config["mcp_servers"] = json!({ "biorouter": { "url": url } });
            // ⚠ Tell the model which set of tools actually works, because the two
            // it can see are not equally usable and the broken pair looks more
            // familiar.
            //
            // Codex's own `exec` and `apply_patch` cannot be removed (only the
            // sandbox constrains them), so a Codex child sees its built-ins AND
            // Biorouter's bridged tools. Observed in the running app: asked to fix
            // a file, it reached for `apply_patch`, hit the read-only sandbox, and
            // reported "this session's filesystem is read-only" — never trying
            // `developer__text_editor`, which would have worked, because the bridge
            // executes in Biorouter's process rather than inside Codex's sandbox.
            // It had diagnosed the bug correctly and still could not apply it.
            //
            // `developerInstructions` rather than appending to `baseInstructions`:
            // the base prompt is Biorouter's and is shared with every other
            // provider, while this is operational guidance about one child's tool
            // surface. Claude Code needs no equivalent because `--tools ""` removes
            // its built-ins outright, leaving nothing to choose wrongly between.
            params["developerInstructions"] = json!(
                "Your own built-in file and shell tools run in a read-only sandbox and cannot \
                 change anything on disk. Do not use them to read or edit files, and do not \
                 report the filesystem as read-only. Use the `biorouter` MCP tools instead — \
                 they run outside that sandbox, and they are how you read files, edit files and \
                 run commands here."
            );
        }
        params["config"] = config;
        params
    }

    /// `turn/start` parameters.
    ///
    /// BR-63's reasoning effort belongs **here** and nowhere else: `thread/start`
    /// has no `effort` field at all (`codex app-server generate-json-schema`),
    /// so a provider that only shapes the thread leaves `/effort` a silent no-op.
    ///
    /// The schema types `effort` as `ReasoningEffort`, which it declares as an
    /// open non-empty string — "a reasoning effort value advertised by the
    /// model" — rather than a closed enum, so the accepted set is per-model and
    /// read from `model/list`. Measured against codex-cli 0.147.0, every model
    /// it lists advertises `low` and `high`; `xhigh` is on all of them too,
    /// `max`/`ultra` only on some. Biorouter therefore sends the same
    /// `low`/`high` pair `provider_effort()` gives every other provider, which
    /// is the subset no model can reject.
    ///
    /// ⚠ `Normal` and `None` must omit the key entirely. `Normal` is documented
    /// as a strict no-op and is the default, so sending `"medium"` for it would
    /// override each model's own `defaultReasoningEffort` — which is not
    /// uniformly medium (gpt-5.6-sol defaults to `low`, gpt-5.3-codex-spark to
    /// `high`) — on every turn of every user who never touched `/effort`.
    fn turn_params(
        thread_id: &str,
        prompt: &str,
        effort: Option<ReasoningEffort>,
        model: &str,
    ) -> Value {
        json!({
            "threadId": thread_id,
            "input": [{ "type": "text", "text": prompt }],
            // Always sent, and on Codex's own per-model ladder rather than the
            // OpenAI-family low/high pair — see `coding_agent::effort`. The model
            // is needed because that ladder differs between models: `max` exists
            // only on part of the 5.6 family, and Biorouter's own four advertised
            // models stop at `xhigh`.
            "effort": crate::providers::coding_agent::effort::codex_effort(effort, model),
        })
    }

    /// Refuse to continue if the run is not actually on the ChatGPT subscription.
    ///
    /// The sibling `ClaudeCodeProvider` has done this on every turn since it
    /// landed, off the `apiKeySource` field in Claude Code's `system/init` frame,
    /// and the reason is the same one here: `agent_env::configure_subscription_child`
    /// strips API credentials out of the child's environment, so reaching a
    /// metered mode is already a defect — but a defect that bills the user's
    /// OpenAI account instead of the subscription they chose, silently and on
    /// every turn until someone reads an invoice. Codex had no equivalent. The
    /// only place `auth_mode` was read was `discovery::probe_codex_auth`, which
    /// feeds the settings card and never runs during a turn, so a `codex` in
    /// api-key mode was simply metered.
    ///
    /// The truth is available on the turn path: the app server answers
    /// `account/read` with `{"account": {"type": "chatgpt" | "apiKey" |
    /// "amazonBedrock", …}}` (measured against codex-cli 0.147.0, which returned
    /// the live `chatgpt` account on this machine). That is one extra JSON-RPC
    /// round trip on the connection the turn is already holding, taken before
    /// `thread/start`, so a refused turn costs no tokens at all.
    ///
    /// Deliberately NOT read from `~/.codex/auth.json` the way discovery does: the
    /// file is what the app server *would* load, but the running process is what
    /// actually bills. `CODEX_HOME`, a login that happened after the file was
    /// read, and an install that authenticates some other way all separate the
    /// two, and this check exists precisely for the case where something outside
    /// Biorouter's expectations is supplying credentials.
    ///
    /// `None` is Ok, matching `ClaudeCodeProvider::assert_subscription_auth`'s
    /// treatment of a missing `apiKeySource`. A build that does not report an
    /// account — an older app server without `account/read`, or one whose answer
    /// omits it — has told us nothing, and turning "nothing" into a refusal would
    /// break every such install for a suspicion. The metered case reports itself
    /// explicitly; it is not something we have to infer from silence.
    fn assert_subscription_auth(account: Option<&Value>) -> Result<(), ProviderError> {
        let kind = account
            .filter(|a| !a.is_null())
            .and_then(|a| a.get("type"))
            .and_then(Value::as_str);
        match kind {
            None | Some("chatgpt") => Ok(()),
            Some(other) => Err(ProviderError::Authentication(format!(
                "This run would have been billed to your Codex `{other}` account rather than to \
                 your ChatGPT subscription, so it was stopped.\n\nBiorouter removes API \
                 credentials from the environment it starts `codex` in, so something outside \
                 that environment is supplying one — most likely an `OPENAI_API_KEY` in a Codex \
                 config file, or a `codex login --api-key` that replaced the subscription \
                 sign-in. Run `codex login` to go back to the subscription."
            ))),
        }
    }

    /// Ask the live app server which account it will bill, and stop if it is not
    /// the subscription.
    ///
    /// A failed *request* is not a failed check. An app server predating
    /// `account/read` rejects the method, and a turn must not die because this
    /// build cannot answer a question about itself — that would take a
    /// working-but-old Codex away from the user to protect them from a billing
    /// mode it never reported. What the response says, when there is one, is
    /// binding; that it could not be obtained is not evidence of anything. The
    /// asymmetry is the same one `assert_subscription_auth` applies to a missing
    /// `account` field, and is why the failure is logged rather than swallowed
    /// silently.
    ///
    /// **Not answering at all is also a failed request**, and it is the one the
    /// fail-open argument above does not cover on its own. "The method was
    /// rejected" is a response; "the method was ignored" is silence, and
    /// `AppServer::request` waits on silence until the child's stdout closes —
    /// which for a healthy-but-unfamiliar app server is never. That would hang
    /// the turn on its first round trip, before `thread/start`, with no error and
    /// no output: a build old enough to lack `account/read` would look like a
    /// Biorouter that stopped working rather than a Codex that stopped answering.
    /// [`ACCOUNT_READ_TIMEOUT`] turns that into the same fail-open branch a
    /// rejection takes.
    async fn assert_subscription(server: &AppServer) -> Result<(), ProviderError> {
        let answered = tokio::time::timeout(
            ACCOUNT_READ_TIMEOUT,
            server.request("account/read", json!({})),
        )
        .await;
        match answered {
            Ok(Ok(response)) => Self::assert_subscription_auth(response.get("account")),
            Ok(Err(e)) => {
                tracing::debug!(
                    error = %e,
                    "codex app-server did not answer account/read; continuing without the \
                     subscription check"
                );
                Ok(())
            }
            Err(_) => {
                tracing::debug!(
                    timeout_secs = ACCOUNT_READ_TIMEOUT.as_secs(),
                    "codex app-server ignored account/read; continuing without the \
                     subscription check rather than holding the turn open"
                );
                Ok(())
            }
        }
    }

    /// Answer a server-originated request.
    ///
    /// Anything that would let the child act on the machine is refused. The child
    /// is configured with a read-only sandbox; its own `exec` and `apply_patch`
    /// remain (they cannot be switched off — see `thread_params`), but the sandbox
    /// is what keeps them from writing. So an approval request here means it is
    /// reaching for authority it was not given, and the honest answer is no.
    /// Elicitation is accepted because that is how an MCP tool call Biorouter
    /// itself is serving gets its go-ahead — and those run
    /// in Biorouter's dispatcher, behind Biorouter's gates.
    /// ⚠ **Each of these five methods wants a DIFFERENT response shape**, and
    /// they are not interchangeable. Every one of them used to be answered with
    /// `{"decision": "denied"}`, and `denied` is not a valid value for any of
    /// them — verified against `codex app-server generate-json-schema` (0.147.0):
    ///
    /// | Method | Response type | Refusal |
    /// |---|---|---|
    /// | `item/commandExecution/requestApproval` | `CommandExecutionApprovalDecision` | `"decline"` (`accept`/`acceptForSession`/`acceptWithExecpolicyAmendment`/`applyNetworkPolicyAmendment`/`decline`/`cancel`) |
    /// | `item/fileChange/requestApproval` | `FileChangeApprovalDecision` | `"decline"` |
    /// | `item/permissions/requestApproval` | **not a decision at all** — `{permissions, scope?, strictAutoReview?}` | an empty `GrantedPermissionProfile`: grant nothing |
    /// | `execCommandApproval`, `applyPatchApproval` (legacy) | `ReviewDecision` | the *object* form `{"denied": {"rejection": …}}` (the bare string is `"abort"`, which also ends the turn) |
    ///
    /// `decline` rather than `cancel`, and `denied` rather than `abort`, on
    /// purpose: both refuse the action while letting the turn continue, so the
    /// child can say why it could not proceed instead of the turn dying silently.
    fn decide(method: &str) -> Value {
        match method {
            "mcpServer/elicitation/request" => json!({ "action": "accept", "content": {} }),
            "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
                json!({ "decision": "decline" })
            }
            // Granting no additional permissions IS the refusal here; there is no
            // decision field to say no with.
            "item/permissions/requestApproval" => json!({ "permissions": {} }),
            "applyPatchApproval" | "execCommandApproval" => json!({
                "decision": {
                    "denied": {
                        "rejection": "Biorouter runs the child read-only; tools run on \
                                      Biorouter's side of the bridge instead."
                    }
                }
            }),
            // An unrecognised request still has to be answered or the turn stalls
            // forever. Refuse rather than guess, in the commonest shape.
            _ => json!({ "decision": "decline" }),
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
        model: &ModelConfig,
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
        model: &ModelConfig,
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

        // Before `thread/start`, so a metered run is refused without sending the
        // user's prompt anywhere or costing a token. `initialize` has to come
        // first — the app server rejects requests before the handshake — which is
        // the only reason this is not the very first thing the turn does.
        Self::assert_subscription(server).await?;

        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| ".".to_string());
        let thread = server
            .request(
                "thread/start",
                Self::thread_params(
                    system,
                    &cwd,
                    &model.model_name,
                    bridge::active_bridge_url().as_deref(),
                ),
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
            Self::turn_params(
                &thread_id,
                prompt,
                model.reasoning_effort,
                &model.model_name,
            ),
        );
        let pump = Self::pump(server);

        let (started, outcome) =
            tokio::time::timeout(TURN_TIMEOUT, async { tokio::join!(start, pump) })
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

    /// The streaming turn: handshake, thread, turn, then pump events out as they
    /// arrive rather than folding them into one final answer.
    ///
    /// Structurally the same as [`Self::turn_on`] up to `turn/start`; from there
    /// every notification goes through [`codex_stream::CodexDecoder`] and each
    /// decoded event is sent on immediately.
    #[allow(clippy::too_many_arguments)]
    async fn stream_turn(
        server: &AppServer,
        model: &ModelConfig,
        system: &str,
        prompt: &str,
        bridge_url: Option<&str>,
        tx: &tokio::sync::mpsc::UnboundedSender<Result<ProviderStreamItem, ProviderError>>,
    ) -> Result<(), ProviderError> {
        server
            .request(
                "initialize",
                json!({
                    "clientInfo": { "name": "biorouter", "version": env!("CARGO_PKG_VERSION") },
                    "capabilities": { "experimentalApi": true }
                }),
            )
            .await?;
        server.notify("initialized", Value::Null).await?;

        // Before `thread/start`, so a metered run is refused without sending the
        // user's prompt anywhere or costing a token.
        Self::assert_subscription(server).await?;

        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| ".".to_string());
        let thread = server
            .request(
                "thread/start",
                Self::thread_params(system, &cwd, &model.model_name, bridge_url),
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

        let start = server.request(
            "turn/start",
            Self::turn_params(&thread_id, prompt, model.reasoning_effort, &model.model_name),
        );
        let pump = Self::stream_pump(server, &model.model_name, tx);

        // The ceiling, inside the stream: the blocking path wraps this same join
        // but that path is not reached here.
        let (started, pumped) =
            tokio::time::timeout(TURN_TIMEOUT, async { tokio::join!(start, pump) })
                .await
                .map_err(|_| {
                    ProviderError::ExecutionError(format!(
                        "the Codex turn did not finish within {}s and was stopped",
                        TURN_TIMEOUT.as_secs()
                    ))
                })?;
        started?;
        pumped
    }

    /// Read notifications, decode them, and forward each decoded event.
    async fn stream_pump(
        server: &AppServer,
        model_name: &str,
        tx: &tokio::sync::mpsc::UnboundedSender<Result<ProviderStreamItem, ProviderError>>,
    ) -> Result<(), ProviderError> {
        let mut decoder = codex_stream::CodexDecoder::new();
        let mut streamed_anything = false;

        while let Some(message) = server.next_inbound().await {
            match message {
                Inbound::Request { id, method, .. } => {
                    server.respond(&id, Self::decide(&method)).await?;
                }
                Inbound::Notification { method, params } => {
                    for event in decoder.push(&method, &params) {
                        match event {
                            codex_stream::CodexEvent::TextDelta { item_id, text }
                            | codex_stream::CodexEvent::TextComplete { item_id, text } => {
                                streamed_anything = true;
                                // The item id becomes the message id so every
                                // chunk of one answer merges into a single row
                                // instead of one row per token.
                                let mut message = Message::new(
                                    Role::Assistant,
                                    chrono::Utc::now().timestamp(),
                                    vec![MessageContent::text(text)],
                                );
                                message.id = Some(item_id);
                                if tx.send(Ok((Some(message), None, None))).is_err() {
                                    return Ok(());
                                }
                            }
                            codex_stream::CodexEvent::ReasoningDelta { item_id, text } => {
                                let mut message = Message::new(
                                    Role::Assistant,
                                    chrono::Utc::now().timestamp(),
                                    // No signature: Codex does not sign its
                                    // reasoning, and a blank one keeps this off
                                    // the signed-turn persistence path.
                                    vec![MessageContent::thinking(text, "")],
                                );
                                message.id = Some(item_id);
                                if tx.send(Ok((Some(message), None, None))).is_err() {
                                    return Ok(());
                                }
                            }
                            codex_stream::CodexEvent::Terminal(terminal) => {
                                if let Some(error) = terminal.error {
                                    return Err(ProviderError::RequestFailed(format!(
                                        "{error}{}",
                                        unknown_model_hint(model_name)
                                    )));
                                }
                                // The authoritative usage goes last, so it is the
                                // snapshot the agent keeps.
                                let usage = decoder
                                    .usage()
                                    .map(|u| u.usage.clone())
                                    .unwrap_or_default();
                                let mut usage = ProviderUsage::new(model_name.to_string(), usage);
                                usage.provider = Some(KIND.provider_id().to_string());
                                let _ = tx.send(Ok((None, Some(usage), None)));
                                return Ok(());
                            }
                            // Parked for phases 3 and 4; notices are advisory and
                            // never end a turn.
                            codex_stream::CodexEvent::Tool(_)
                            | codex_stream::CodexEvent::Usage(_)
                            | codex_stream::CodexEvent::Notice { .. }
                            | codex_stream::CodexEvent::Ignored => {}
                        }
                    }
                }
            }
        }

        // stdout closed without a terminal frame.
        if streamed_anything {
            Ok(())
        } else {
            Err(ProviderError::RequestFailed(format!(
                "the Codex app server exited before finishing the turn{}",
                server.stderr_suffix().await
            )))
        }
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

        let outcome = self.run_turn(model_config, system, &prompt).await?;

        if outcome.text.is_empty() {
            let detail = outcome
                .failure
                .unwrap_or_else(|| "Codex returned an empty response".to_string());
            let detail = format!("{detail}{}", unknown_model_hint(&model_config.model_name));
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

    fn supports_streaming(&self) -> bool {
        true
    }

    /// Stream one turn: text appears as the model writes it.
    ///
    /// The app server already sends everything needed for this — the previous
    /// implementation simply threw it away, keeping only the one `item/completed`
    /// frame that arrives after the model has finished. [`codex_stream`] decodes
    /// the delta notifications instead; this method owns the process, the
    /// handshake and the pump.
    ///
    /// The same three rules as the Claude path apply, for the same reasons:
    /// the bridge URL is read **here**, at construction, because the task-local
    /// scope is gone once the stream is polled; the app server is owned by a task
    /// the stream aborts on drop, so a cancelled turn cannot leave one running
    /// with the user's credential; and the turn ceiling lives inside the stream,
    /// because the blocking path's timeout wraps a join this path never reaches.
    async fn stream(
        &self,
        system: &str,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let prompt = transcript::flatten(messages).ok_or_else(|| {
            ProviderError::RequestFailed("there is no user message for Codex to answer".to_string())
        })?;

        // Read INSIDE the scope, before the stream exists. `thread_params` needs
        // it, and by the time the stream is polled `active_bridge_url()` is None.
        let bridge_url = bridge::active_bridge_url();
        let model_config = self.model.clone();
        let command = self.app_server_command();
        // Owned copies: the pump outlives this call's borrows.
        let system = system.to_string();

        let (tx, rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<ProviderStreamItem, ProviderError>>();

        let pump = tokio::spawn(async move {
            let server = match AppServer::spawn(command).await {
                Ok(server) => server,
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                }
            };

            let outcome = Self::stream_turn(
                &server,
                &model_config,
                &system,
                &prompt,
                bridge_url.as_deref(),
                &tx,
            )
            .await;

            if let Err(e) = outcome {
                let _ = tx.send(Err(e));
            }

            // Always reap: a leaked `codex app-server` is a live process holding
            // the user's credential.
            server.shutdown().await;
        });

        let guard = coding_agent::AbortOnDrop(pump.abort_handle());
        let stream = async_stream::try_stream! {
            let _guard = guard;
            let mut rx = rx;
            while let Some(item) = rx.recv().await {
                yield item?;
            }
        };

        Ok(Box::pin(stream))
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

    /// Only a `chatgpt` account is the subscription. Everything else is metered,
    /// and a metered run is refused rather than billed quietly.
    ///
    /// The three named account types are the app server's own closed set
    /// (`GetAccountResponse` in `codex app-server generate-json-schema`), and the
    /// unknown-string case is here because that set is the vendor's to extend: a
    /// type this build has never heard of must not fall through to "fine".
    #[test]
    fn only_a_chatgpt_account_is_the_subscription() {
        assert!(
            CodexProvider::assert_subscription_auth(Some(&json!({"type": "chatgpt"}))).is_ok(),
            "the ChatGPT subscription is the whole point of this provider"
        );

        for metered in ["apiKey", "amazonBedrock", "somethingNewFromOpenAI"] {
            let err = CodexProvider::assert_subscription_auth(Some(&json!({"type": metered})))
                .expect_err("a non-subscription account must stop the turn");
            assert!(
                matches!(err, ProviderError::Authentication(_)),
                "a billing-mode refusal must be typed Authentication so the retry \
                 layer does not retry it: {err:?}"
            );
            assert!(
                err.to_string().contains(metered),
                "the refusal must name what would have been billed: {err}"
            );
        }
    }

    /// No account reported is not evidence of a metered run.
    ///
    /// An app server predating `account/read`, or one whose answer omits the
    /// field, has told us nothing — and turning "nothing" into a refusal would
    /// take a working Codex away from the user over a suspicion. This mirrors
    /// `ClaudeCodeProvider::assert_subscription_auth`, which treats a missing
    /// `apiKeySource` the same way. The metered case announces itself.
    #[test]
    fn an_unreported_account_does_not_stop_the_turn() {
        assert!(CodexProvider::assert_subscription_auth(None).is_ok());
        assert!(CodexProvider::assert_subscription_auth(Some(&Value::Null)).is_ok());
        assert!(CodexProvider::assert_subscription_auth(Some(&json!({}))).is_ok());
    }

    /// The check is ON THE TURN PATH, and it fires before the prompt is sent.
    ///
    /// This is the half that the pure tests above cannot reach and the half that
    /// was actually missing: `auth_mode` was already readable, just only from
    /// `discovery::probe_codex_auth`, which builds the settings card and never
    /// runs during a turn. A correct `assert_subscription_auth` that nothing
    /// calls bills the user exactly as much as no check at all.
    ///
    /// The fake app server below answers `account/read` with an `apiKey` account
    /// and would happily complete a turn if asked. So "the turn failed with an
    /// Authentication error" and "`thread/start` was never reached" are the same
    /// observation from two sides, and the second is what proves the refusal
    /// costs no tokens.
    // ⚠ `cfg(unix)`: this test drives a fake app server written in Python over
    // stdio, and CI installs no Python on Windows (there is no `setup-python`
    // step in `.github/workflows/rust.yml`). On Windows `python3` resolves to
    // the Store's App Execution Alias or to nothing, the child never answers,
    // and `AppServer::request` — which by design has no timeout of its own, see
    // its doc comment — awaits its oneshot forever. That does not fail the job,
    // it HANGS it: `test (windows-latest)` ran 120+ minutes against a ~20 minute
    // norm until the runner's ceiling, on every head carrying this code. The
    // transport semantics under test are not platform-specific; the fake server
    // is.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_metered_codex_is_refused_before_the_prompt_is_sent() {
        let server = fake_app_server(r#"{"type":"apiKey"}"#).await;
        let provider = test_provider();

        let err = provider
            .turn_on(
                &server,
                &ModelConfig::new_or_fail("gpt-5.5"),
                "SYSTEM",
                "hello",
            )
            .await
            .expect_err("a turn on an api-key Codex must not run");

        assert!(
            matches!(err, ProviderError::Authentication(_)),
            "expected an Authentication refusal, got {err:?}"
        );
        assert!(
            err.to_string().contains("apiKey"),
            "the refusal must name the account that would have been billed: {err}"
        );

        let reached = server
            .request("test/reachedThreadStart", json!({}))
            .await
            .expect("the fake server is still alive to be asked");
        assert_eq!(
            reached["value"], false,
            "the prompt must not be sent to a metered account; the refusal has to \
             happen before `thread/start`"
        );
    }

    /// The same fake reporting a `chatgpt` account runs the turn through.
    ///
    /// Without this the test above is satisfied by a check that refuses
    /// everything, which would break the provider for every correctly signed-in
    /// user while looking like a passing suite.
    // ⚠ `cfg(unix)`: this test drives a fake app server written in Python over
    // stdio, and CI installs no Python on Windows (there is no `setup-python`
    // step in `.github/workflows/rust.yml`). On Windows `python3` resolves to
    // the Store's App Execution Alias or to nothing, the child never answers,
    // and `AppServer::request` — which by design has no timeout of its own, see
    // its doc comment — awaits its oneshot forever. That does not fail the job,
    // it HANGS it: `test (windows-latest)` ran 120+ minutes against a ~20 minute
    // norm until the runner's ceiling, on every head carrying this code. The
    // transport semantics under test are not platform-specific; the fake server
    // is.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_subscription_codex_still_runs_its_turn() {
        let server = fake_app_server(r#"{"type":"chatgpt","planType":"pro"}"#).await;
        let provider = test_provider();

        let outcome = provider
            .turn_on(
                &server,
                &ModelConfig::new_or_fail("gpt-5.5"),
                "SYSTEM",
                "hello",
            )
            .await
            .expect("a subscription turn must not be refused");
        assert_eq!(outcome.text, vec!["hi from the fake".to_string()]);
    }

    /// An app server that ignores `account/read` must not hold the turn open.
    ///
    /// The check is documented as fail-open, and the argument for that only
    /// covers a server which *answers* an unknown method with a JSON-RPC error.
    /// Silence is the other way to fail, and `AppServer::request` waits on
    /// silence until the child's stdout closes — so without a timeout this is not
    /// a degraded check, it is a turn that never starts: no error, no output,
    /// nothing to cancel it, on the very first round trip and before
    /// `thread/start`. A build too old to know the method would present as a
    /// Biorouter that stopped working.
    ///
    /// The fake below answers `initialize` and ignores everything else, which is
    /// exactly the shape being defended against — a healthy server that simply
    /// does not know this method. The clock is paused so the real
    /// [`ACCOUNT_READ_TIMEOUT`] elapses in virtual time; the test therefore
    /// exercises the production constant rather than a shortened copy of it, and
    /// still finishes in milliseconds.
    ///
    /// The outer budget is three times the inner one, so removing the `timeout`
    /// from `assert_subscription` fails here instead of hanging the suite.
    // ⚠ `cfg(unix)`: this test drives a fake app server written in Python over
    // stdio, and CI installs no Python on Windows (there is no `setup-python`
    // step in `.github/workflows/rust.yml`). On Windows `python3` resolves to
    // the Store's App Execution Alias or to nothing, the child never answers,
    // and `AppServer::request` — which by design has no timeout of its own, see
    // its doc comment — awaits its oneshot forever. That does not fail the job,
    // it HANGS it: `test (windows-latest)` ran 120+ minutes against a ~20 minute
    // norm until the runner's ceiling, on every head carrying this code. The
    // transport semantics under test are not platform-specific; the fake server
    // is.
    #[cfg(unix)]
    #[tokio::test(start_paused = true)]
    async fn an_app_server_that_ignores_account_read_does_not_hang_the_turn() {
        let server = silent_app_server().await;

        let checked = tokio::time::timeout(
            ACCOUNT_READ_TIMEOUT * 3,
            CodexProvider::assert_subscription(&server),
        )
        .await;

        let verdict = checked.expect(
            "assert_subscription never returned: an app server that ignores \
             account/read holds the turn open forever, because AppServer::request \
             waits on its oneshot until the child's stdout closes",
        );
        assert!(
            verdict.is_ok(),
            "a check that could not be obtained is not evidence of a metered run; \
             a timeout must fail open exactly as a rejected method does: {verdict:?}"
        );
    }

    /// A stand-in for an app server that answers what it knows and silently
    /// ignores what it does not — i.e. any build predating `account/read`.
    #[cfg(unix)]
    async fn silent_app_server() -> AppServer {
        let script = r#"
import sys, json
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    m = json.loads(line)
    if m.get("method") == "initialize":
        print(json.dumps({"jsonrpc":"2.0","id":m["id"],"result":{"codexHome":"/tmp"}}), flush=True)
    # Everything else, account/read included, is read and dropped on the floor.
"#;
        let mut cmd = tokio::process::Command::new("python3");
        cmd.arg("-c").arg(script);
        AppServer::spawn(cmd)
            .await
            .expect("the silent app server should start")
    }

    fn test_provider() -> CodexProvider {
        // The command is never spawned: `turn_on` is handed an already-running
        // `AppServer`, which is the seam that makes this testable at all.
        CodexProvider {
            command: PathBuf::from("codex"),
            model: ModelConfig::new_or_fail("gpt-5.5"),
            name: KIND.provider_id().to_string(),
        }
    }

    /// A scripted stand-in for `codex app-server` that reports `account` as the
    /// caller asks and otherwise completes a turn normally.
    ///
    /// Python for the same reason `appserver.rs`'s own fake is: the protocol is
    /// newline-delimited JSON and a real Codex would need a real subscription,
    /// which no test can have. It also answers a synthetic
    /// `test/reachedThreadStart`, which is the only way to observe from outside
    /// that the refusal happened *before* the prompt went anywhere rather than
    /// after.
    #[cfg(unix)]
    async fn fake_app_server(account: &str) -> AppServer {
        let script = format!(
            r#"
import sys, json
reached = False
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    m = json.loads(line)
    method = m.get("method")
    if method == "initialize":
        print(json.dumps({{"jsonrpc":"2.0","id":m["id"],"result":{{"codexHome":"/tmp"}}}}), flush=True)
    elif method == "account/read":
        print(json.dumps({{"jsonrpc":"2.0","id":m["id"],
                           "result":{{"account":{account},"requiresOpenaiAuth":True}}}}), flush=True)
    elif method == "thread/start":
        reached = True
        print(json.dumps({{"jsonrpc":"2.0","id":m["id"],"result":{{"thread":{{"id":"t-1"}}}}}}), flush=True)
    elif method == "turn/start":
        print(json.dumps({{"jsonrpc":"2.0","method":"item/completed",
                           "params":{{"item":{{"type":"agentMessage","text":"hi from the fake"}}}}}}), flush=True)
        print(json.dumps({{"jsonrpc":"2.0","method":"turn/completed","params":{{"usage":{{}}}}}}), flush=True)
        print(json.dumps({{"jsonrpc":"2.0","id":m["id"],"result":{{}}}}), flush=True)
    elif method == "test/reachedThreadStart":
        print(json.dumps({{"jsonrpc":"2.0","id":m["id"],"result":{{"value":reached}}}}), flush=True)
"#
        );
        let mut cmd = tokio::process::Command::new("python3");
        cmd.arg("-c").arg(script);
        AppServer::spawn(cmd)
            .await
            .expect("the fake app server should start")
    }

    /// The thread is read-only, ephemeral, and carries Biorouter's instructions.
    /// Each of the four is a decision rather than a default, so each is pinned.
    #[test]
    fn thread_params_are_locked_down() {
        let p = CodexProvider::thread_params("SYSTEM", "/tmp/work", "gpt-5.5", None);
        assert_eq!(
            p["sandbox"], "read-only",
            "the child must not be able to write"
        );
        assert_eq!(
            p["ephemeral"], true,
            "Codex must not persist its own transcript"
        );
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
            without["config"].get("mcp_servers").is_none(),
            "no bridge must mean no server map"
        );
        // …but the web tool is disabled either way: that is not part of the
        // bridge, it is a standing restriction on the child.
        assert_eq!(without["config"]["tools"]["web_search"], false);
        assert_eq!(with["config"]["tools"]["web_search"], false);
    }

    /// The turn's prompt and thread id are always sent — the effort rides
    /// alongside them and must not displace either.
    #[test]
    fn turn_params_always_carry_the_thread_and_the_prompt() {
        let p = CodexProvider::turn_params("th_1", "why?", Some(ReasoningEffort::Deep), "gpt-5.5");
        assert_eq!(p["threadId"], "th_1");
        assert_eq!(p["input"][0]["type"], "text");
        assert_eq!(p["input"][0]["text"], "why?");
    }

    /// `/effort` has to arrive on `turn/start`, or it is a silent no-op:
    /// `thread/start` declares no `effort` field at all, so shaping the thread
    /// cannot carry it.
    ///
    /// The rungs are Codex's own ladder rather than the OpenAI-family `low`/`high`
    /// pair — `coding_agent::effort` owns the table and the reasoning. `Deep`
    /// stops at `xhigh` here because `gpt-5.5` does not advertise `max`.
    #[test]
    fn the_effort_ladder_reaches_the_turn() {
        for (effort, expected) in [
            (Some(ReasoningEffort::Quick), "low"),
            (Some(ReasoningEffort::Normal), "high"),
            (Some(ReasoningEffort::Deep), "xhigh"),
            (None, "high"),
        ] {
            let p = CodexProvider::turn_params("th_1", "hi", effort, "gpt-5.5");
            assert_eq!(
                p["effort"], expected,
                "{effort:?} must reach the app server as effort={expected}"
            );
        }
    }

    /// The effort is **always** sent, including for the default. That is a
    /// deliberate departure from every other provider, where `Normal` is silence:
    /// a coding agent is reached for when the work is hard, so Biorouter's default
    /// here is `high` rather than whatever the model would have chosen. Asserted
    /// explicitly because it costs the user thinking tokens on every turn.
    #[test]
    fn the_default_effort_is_high_rather_than_silence() {
        let p = CodexProvider::turn_params("th_1", "hi", None, "gpt-5.5");
        assert_eq!(p["effort"], "high");
        assert!(
            !p["effort"].is_null(),
            "an explicit null is a value sent, not an absence"
        );
    }

    /// Codex's ladder is per-model, so the same `/effort deep` reaches a different
    /// rung on a model that advertises `max`.
    #[test]
    fn deep_follows_the_models_own_ladder() {
        let short = CodexProvider::turn_params("t", "hi", Some(ReasoningEffort::Deep), "gpt-5.5");
        let tall =
            CodexProvider::turn_params("t", "hi", Some(ReasoningEffort::Deep), "gpt-5.6-sol");
        assert_eq!(short["effort"], "xhigh");
        assert_eq!(tall["effort"], "max");
    }

    /// The child is told which of its two tool surfaces actually works.
    ///
    /// Without this it reaches for its own `apply_patch`, hits the read-only
    /// sandbox and reports the filesystem as read-only — observed in the running
    /// app on a task it had already diagnosed correctly. Only sent when there is a
    /// bridge, because without one the advice would point at tools that are not
    /// there.
    #[test]
    fn a_bridged_child_is_steered_away_from_its_crippled_built_ins() {
        let with =
            CodexProvider::thread_params("S", "/tmp", "gpt-5.5", Some("http://x/tool_bridge/n"));
        let advice = with["developerInstructions"].as_str().unwrap_or_default();
        assert!(
            advice.contains("biorouter"),
            "it must name the tools that work: {advice}"
        );
        assert!(
            advice.contains("read-only"),
            "and explain why its own are not usable: {advice}"
        );

        let without = CodexProvider::thread_params("S", "/tmp", "gpt-5.5", None);
        assert!(
            without.get("developerInstructions").is_none(),
            "with no bridge there are no Biorouter tools to point at"
        );
    }

    /// Every approval that would let the child act on the machine is refused;
    /// elicitation — how a Biorouter-served MCP tool call is cleared — is accepted.
    ///
    /// ⚠ This test used to assert `decision == "denied"` for all five methods.
    /// That is not a valid value for **any** of them, so the refusals were being
    /// sent in a shape the app server cannot parse, and the test agreed with the
    /// bug rather than catching it. Each refusal below is now the one its own
    /// response schema defines (`codex app-server generate-json-schema`, 0.147.0)
    /// — which is three different shapes, not one.
    #[test]
    fn only_elicitation_is_accepted() {
        assert_eq!(
            CodexProvider::decide("mcpServer/elicitation/request")["action"],
            "accept"
        );

        // `*ApprovalDecision`: a plain string. `decline` refuses the action and
        // lets the turn continue; `cancel` would kill the turn.
        for refused in [
            "item/commandExecution/requestApproval",
            "item/fileChange/requestApproval",
        ] {
            assert_eq!(
                CodexProvider::decide(refused)["decision"],
                "decline",
                "{refused} takes a *ApprovalDecision, whose refusal is `decline`"
            );
        }

        // Not a decision at all: the response IS the permission grant, so
        // granting nothing is how it is refused.
        let permissions = CodexProvider::decide("item/permissions/requestApproval");
        assert!(
            permissions
                .get("permissions")
                .and_then(Value::as_object)
                .is_some_and(serde_json::Map::is_empty),
            "item/permissions/requestApproval takes {{permissions: GrantedPermissionProfile}} \
             — an empty profile grants nothing, which is the refusal; a `decision` \
             field here is not part of the schema at all (got {permissions})"
        );

        // Legacy `ReviewDecision`: the refusal that continues the turn is the
        // OBJECT form. The bare string `denied` is not in the enum.
        for legacy in ["applyPatchApproval", "execCommandApproval"] {
            let answer = CodexProvider::decide(legacy);
            assert!(
                answer["decision"]["denied"]["rejection"].is_string(),
                "{legacy} takes a ReviewDecision, whose continue-the-turn refusal \
                 is {{denied: {{rejection}}}} (got {answer})"
            );
            assert!(
                !answer["decision"].is_string(),
                "{legacy} must not be answered with a bare string — `denied` is not \
                 one of the enum's string forms (those are `approved`, \
                 `approved_for_session`, `timed_out`, `abort`)"
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
            (
                "item/completed",
                json!({"item":{"id":"i0","type":"userMessage"}}),
            ),
            (
                "item/completed",
                json!({"item":{"id":"i1","type":"reasoning"}}),
            ),
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
        assert!(
            !ended,
            "an advisory error precedes turn.started and is not fatal"
        );
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

    /// An unknown model must not fail anonymously.
    ///
    /// Codex reports one as an `error` notification carrying no message and no
    /// category, so the turn reaches the user as "the Codex app server reported
    /// an error" plus the generic invitation to retry — an instruction that can
    /// never come true for a request that will fail identically forever. This
    /// is the one thing the provider knows and the app server does not: which
    /// names it believes Codex offers.
    #[test]
    fn a_failed_turn_names_an_unknown_model_and_the_ones_that_exist() {
        let hint = unknown_model_hint("gpt-5.5-codex");
        assert!(
            hint.contains("gpt-5.5-codex"),
            "the hint must name the model that was asked for: {hint}"
        );
        assert!(
            hint.contains("gpt-5.5") && hint.contains("gpt-5.3-codex"),
            "and the ones that exist, so the fix is in the message: {hint}"
        );
        assert!(
            hint.contains("no retry will fix it"),
            "and must contradict the retry advice it is appended to: {hint}"
        );
    }

    /// ⚠ And it must stay SILENT for a model that is known, or every unrelated
    /// failure — a rate limit, a dropped connection — gains a paragraph about
    /// model names and sends the reader after the wrong thing.
    #[test]
    fn a_known_model_adds_nothing_to_a_failure() {
        for m in known_models() {
            assert_eq!(
                unknown_model_hint(&m.name),
                "",
                "{} is a declared model and must not be second-guessed",
                m.name
            );
        }
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
        assert!(
            !m.runs_locally,
            "the subprocess is local but the inference is not"
        );
        assert_eq!(m.config_keys.len(), 1);
        assert_eq!(m.config_keys[0].name, "CODEX_COMMAND");
        assert!(m.config_keys[0].required);
        assert!(!m.config_keys[0].secret);
        assert_eq!(m.config_keys[0].default.as_deref(), Some("codex"));
    }
}

/// Phase 2 end-to-end: the Codex streaming path, driven by a fake `codex
/// app-server` that emits the delta notifications a real one does.
///
/// The unit tests in `codex_stream` prove the decoder against recorded frames;
/// these prove the wiring around it — the handshake, the pump, the message ids
/// that make chunks merge into one row, and the usage attribution.
#[cfg(all(test, unix))]
mod streaming_tests {
    use super::*;
    use futures::StreamExt;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    /// A stand-in for the `codex` binary. It ignores the `app-server` argument
    /// and speaks just enough of the protocol to carry one streamed turn.
    ///
    /// `TOKEN_USAGE_FRAMES` is the interesting part: two snapshots, exactly as a
    /// tool-using turn produces, so the test can prove the provider reports the
    /// cumulative `total` and not the per-request `last`.
    fn fake_codex() -> tempfile::NamedTempFile {
        let script = r#"#!/usr/bin/env python3
import sys, json

def send(obj):
    print(json.dumps(obj), flush=True)

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    m = json.loads(line)
    method = m.get("method")
    if method == "initialize":
        send({"jsonrpc":"2.0","id":m["id"],"result":{"codexHome":"/tmp"}})
    elif method == "account/read":
        send({"jsonrpc":"2.0","id":m["id"],
              "result":{"account":{"type":"chatgpt","planType":"pro"},
                        "requiresOpenaiAuth":True}})
    elif method == "thread/start":
        send({"jsonrpc":"2.0","id":m["id"],"result":{"thread":{"id":"t-1"}}})
    elif method == "turn/start":
        send({"jsonrpc":"2.0","method":"item/started",
              "params":{"threadId":"t-1","turnId":"turn-1",
                        "item":{"id":"msg_1","type":"agentMessage","text":""}}})
        for piece in ["Hello", ", ", "world"]:
            send({"jsonrpc":"2.0","method":"item/agentMessage/delta",
                  "params":{"threadId":"t-1","turnId":"turn-1",
                            "itemId":"msg_1","delta":piece}})
        send({"jsonrpc":"2.0","method":"item/completed",
              "params":{"threadId":"t-1","turnId":"turn-1",
                        "item":{"id":"msg_1","type":"agentMessage",
                                "text":"Hello, world"}}})
        # First model request.
        send({"jsonrpc":"2.0","method":"thread/tokenUsage/updated",
              "params":{"threadId":"t-1","tokenUsage":{
                  "last":{"inputTokens":100,"cachedInputTokens":0,
                          "outputTokens":10,"totalTokens":110},
                  "total":{"inputTokens":100,"cachedInputTokens":0,
                           "outputTokens":10,"totalTokens":110}}}})
        # Second model request: `last` resets, `total` accumulates. Reading
        # `last` here would undercount the turn by the first request.
        send({"jsonrpc":"2.0","method":"thread/tokenUsage/updated",
              "params":{"threadId":"t-1","tokenUsage":{
                  "last":{"inputTokens":200,"cachedInputTokens":0,
                          "outputTokens":20,"totalTokens":220},
                  "total":{"inputTokens":300,"cachedInputTokens":0,
                           "outputTokens":30,"totalTokens":330}}}})
        send({"jsonrpc":"2.0","method":"turn/completed",
              "params":{"threadId":"t-1","turn":{"id":"turn-1","status":"completed"}}})
        send({"jsonrpc":"2.0","id":m["id"],"result":{}})
"#;
        let mut file = tempfile::NamedTempFile::new().expect("temp script");
        file.write_all(script.as_bytes()).unwrap();
        file.flush().unwrap();
        let mut perms = std::fs::metadata(file.path()).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(file.path(), perms).unwrap();
        file
    }

    fn provider_running(script: &tempfile::NamedTempFile) -> CodexProvider {
        CodexProvider {
            command: script.path().to_path_buf(),
            model: ModelConfig::new("gpt-5.5").unwrap(),
            name: KIND.provider_id().to_string(),
        }
    }

    async fn drive() -> (Vec<Message>, Vec<ProviderUsage>) {
        let script = fake_codex();
        let provider = provider_running(&script);
        let messages = vec![Message::user().with_text("hello")];

        let stream = provider
            .stream("SYS", &messages, &[])
            .await
            .expect("the stream should open");
        futures::pin_mut!(stream);

        let mut out_messages = Vec::new();
        let mut usages = Vec::new();
        while let Some(item) = stream.next().await {
            let (message, usage, _) = item.expect("no item should error");
            if let Some(message) = message {
                out_messages.push(message);
            }
            if let Some(usage) = usage {
                usages.push(usage);
            }
        }
        (out_messages, usages)
    }

    /// Text arrives as it is written, and every chunk carries the same id so the
    /// store merges them into one row instead of one row per token.
    #[tokio::test]
    async fn a_turn_streams_its_text_in_parts_under_one_message_id() {
        let (messages, _) = drive().await;

        assert!(
            messages.len() > 1,
            "the answer must arrive in parts (got {} message(s))",
            messages.len()
        );

        let ids: std::collections::BTreeSet<_> =
            messages.iter().filter_map(|m| m.id.clone()).collect();
        assert_eq!(
            ids.len(),
            1,
            "every chunk of one answer must share the item id as its message id, \
             or persistence fragments into a row per delta (got {ids:?})"
        );

        let text: String = messages
            .iter()
            .flat_map(|m| m.content.iter())
            .filter_map(|c| match c {
                MessageContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            text, "Hello, world",
            "the streamed pieces must reconstruct the answer exactly once — a \
             completed frame that re-emitted the full text would double it"
        );
    }

    /// The turn's usage is the cumulative `total` of the last snapshot.
    ///
    /// The fake sends two snapshots, as a turn that makes two model requests
    /// does. Reading the final `last` (220) instead of the final `total` (330)
    /// undercounts by the whole first request — silently, and by more the more
    /// tools the turn used.
    #[tokio::test]
    async fn usage_is_the_cumulative_total_not_the_last_request() {
        let (_, usages) = drive().await;

        let last = usages.last().expect("a terminal usage item");
        assert_eq!(
            last.usage.total_tokens,
            Some(330),
            "usage must come from tokenUsage.total (330), not tokenUsage.last (220)"
        );
        assert_eq!(
            last.provider.as_deref(),
            Some(KIND.provider_id()),
            "and must be attributed to this provider, or a subscription turn is \
             priced as an API call"
        );
    }
}
