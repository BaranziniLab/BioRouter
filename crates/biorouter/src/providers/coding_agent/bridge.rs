//! Exposing Biorouter's own tools to a child coding agent, over MCP.
//!
//! # The problem this solves
//!
//! `claude` and `codex` are complete agents: they run their own loop and execute
//! their own file and shell tools. Biorouter switches those off, because a tool
//! the child runs itself is invisible to Biorouter's inspectors, permission modes,
//! `.biorouterignore` and vault. But then the child can do nothing — no SPOKE, no
//! OMOP, no knowledge base — which is most of the point of using Biorouter at all.
//!
//! There is exactly one channel that returns a tool *result* into a live turn of
//! either CLI: an MCP server the child itself calls. Neither CLI's permission
//! protocol has an outcome meaning "the host already ran this, here is the
//! result" — Claude Code's `can_use_tool` resolves only to allow (optionally with
//! rewritten input) or deny, and Codex's approvals are approve/deny. So MCP is
//! not one option among several; it is the mechanism.
//!
//! # Why this is generic, and why that matters
//!
//! Nothing here knows anything about any individual tool, and no tool needs
//! per-tool work to become available. That falls out of the fact that both sides
//! already speak MCP: Biorouter's tools *are* `rmcp::model::Tool`, and
//! `ExtensionManager::dispatch_tool_call` already takes MCP's own
//! `CallToolRequestParams`. So this is a relay —
//!
//! ```text
//! MCP tools/list  <-  the session's tool set, exactly as the model would see it
//! MCP tools/call   ->  the inspector stack, then ExtensionManager::dispatch_tool_call
//! ```
//!
//! — and a new extension, a marketplace plugin or a future tool works the moment
//! it loads. Verified against a 60-tool surface: both CLIs accepted a 73-character
//! prefixed name, a schema using `$defs`/`$ref`/`oneOf`, an image result, and a
//! `ui://` embedded resource, all passed through unchanged.
//!
//! # Transport: HTTP, and why the URL is the credential
//!
//! The bridge is an HTTP endpoint on the daemon rather than a spawned stdio
//! server. That avoids inventing a second process and a socket, and it means the
//! child talks to the live `ExtensionManager` rather than to a copy.
//!
//! Both CLIs accept a remote MCP server on loopback HTTP — verified. Claude Code
//! also accepts a `headers` map in its config file, but **Codex sends no
//! Authorization header at all** (observed: `auth=None` on every request). So the
//! capability cannot live in a header. It lives in the URL path as an
//! unguessable, single-turn nonce, which both CLIs transmit by construction.
//!
//! The nonce is also why nothing here reads the daemon's secret key: the child's
//! environment is scrubbed of it (issue #57), and a bridge grant is a far narrower
//! capability than the daemon's REST API — one session's tools, for one turn.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use tokio_util::sync::CancellationToken;

use crate::agents::extension_manager::ExtensionManager;
use crate::config::BioRouterMode;
use crate::conversation::message::ToolRequest;
use crate::conversation::Conversation;
use crate::privacy::CallCapability;
use crate::session::session_manager::Session;
use crate::tool_inspection::ToolInspectionManager;

/// Everything one turn's bridge needs to serve `tools/list` and `tools/call`.
///
/// Deliberately a snapshot rather than a handle back to the `Agent`: the provider
/// is called from inside the agent's own stack and cannot hold a reference to it,
/// and a grant that outlived its turn would be a capability with no owner.
pub struct BridgeGrant {
    session: Session,
    mode: BioRouterMode,
    extensions: Arc<ExtensionManager>,
    inspections: Arc<ToolInspectionManager>,
    /// Sampled ONCE, when the grant is issued, and threaded from there.
    ///
    /// Not re-derived per call: `CallCapability` exists precisely so a call's
    /// privacy capability is fixed before the call runs rather than re-read while
    /// it runs, and a bridged call is a call.
    capability: CallCapability,
    /// The tool set as the model would have seen it — already tier-filtered and
    /// reach-filtered by `filter_tools`, so Gate E is inherited rather than
    /// reimplemented.
    tools: Vec<Tool>,
    /// Conversation snapshot the inspectors read for context.
    conversation: Conversation,
    /// **The turn's own** cancel token, not one made here.
    ///
    /// Every cancellation mechanism Biorouter has reaches a running tool through
    /// the token the agent threads down from the turn: issue #72's nested-shell
    /// kill, `AppState::cancel_turn`, and the `TurnGuard` that fires when a
    /// websocket drops. `ExtensionManager::dispatch_tool_call` takes the token by
    /// value and has no other way to learn that the turn is over, so a token
    /// constructed at the call site is a token nobody holds and nobody will ever
    /// cancel — the tool runs to completion whatever the user does.
    ///
    /// That failure is *worse* on this path than on the agent's own, not merely
    /// equal to it. A bridged call is made by a child process running its own
    /// loop: the user pressing stop tears down the turn and the child, but a
    /// `developer__shell` the child had already started would keep running,
    /// detached, with nothing left to report to. So the token is threaded in from
    /// `Agent::issue_tool_bridge`, which is called from the reply loop where the
    /// turn's token is in scope.
    ///
    /// `Option`, because the agent's own token is `Option<CancellationToken>` —
    /// a turn driven by something that never cancels (a workflow step, a test)
    /// genuinely has none. `unwrap_or_default()` at dispatch, exactly as
    /// `Agent::dispatch_tool_call` does with the same value, so "no token" keeps
    /// meaning "never cancelled" rather than becoming an error.
    cancel: Option<CancellationToken>,
    /// The session's hooks, so a PreToolUse rewrite this grant's own inspection
    /// pass produced can actually be collected.
    ///
    /// The rewrite is staged inside the manager rather than returned from
    /// `inspect_tools`, and the only way to collect it is
    /// [`crate::hooks::HooksManager::take_tool_input_rewrites`], which needs the
    /// manager itself. Without a handle to it the bridge ran the user's hooks —
    /// including their side effects — and then dispatched the arguments the hook
    /// had asked to replace.
    hooks: Arc<crate::hooks::HooksManager>,
    /// BRSDK encryption: the app's secret vault, or `None` for a normal session.
    ///
    /// A snapshot taken when the grant is issued, like every other field here —
    /// `Agent::set_vault` is called once when an app's agent is configured, well
    /// before any turn, so there is nothing for a per-call re-read to observe.
    ///
    /// A grant *without* this resolved nothing, and the failure was silent in the
    /// worst way: a `{{vault:NAME}}` placeholder that reaches a tool is not an
    /// error, it is a string. The tool sends it as an Authorization header, or
    /// writes it into a config file, and the request comes back 401 — a
    /// credential problem with no credential anywhere near it, on a path where
    /// the same call from the same app works fine under any other provider.
    vault: Option<Arc<crate::agents::vault_refs::VaultRefs>>,
}

impl BridgeGrant {
    pub fn new(
        session: Session,
        mode: BioRouterMode,
        extensions: Arc<ExtensionManager>,
        inspections: Arc<ToolInspectionManager>,
        capability: CallCapability,
        tools: Vec<Tool>,
        conversation: Conversation,
        cancel: Option<CancellationToken>,
        hooks: Arc<crate::hooks::HooksManager>,
        vault: Option<Arc<crate::agents::vault_refs::VaultRefs>>,
    ) -> Self {
        Self {
            session,
            mode,
            extensions,
            inspections,
            capability,
            tools,
            conversation,
            cancel,
            hooks,
            vault,
        }
    }

    /// The tools to advertise. Already filtered; no further policy here.
    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }

    pub fn session_id(&self) -> &str {
        &self.session.id
    }

    /// The token a bridged dispatch is handed.
    ///
    /// A named accessor rather than an inline expression at the dispatch site
    /// because this one expression decides whether a running bridged tool is
    /// reachable by "stop" at all, and an inline `CancellationToken::new()` there
    /// is both the bug this replaced and completely invisible to every test — it
    /// type-checks, it dispatches, and the tool runs. Naming it gives a test
    /// something to hold that is the same value production uses.
    fn dispatch_cancel_token(&self) -> CancellationToken {
        self.cancel.clone().unwrap_or_default()
    }

    /// Point the developer server's `text_editor` path jail at **this grant's**
    /// mode, before anything is dispatched.
    ///
    /// `biorouter_mcp::set_path_jail_relaxed` is a process-global atomic with a
    /// single production setter: the top of `Agent::inspect_and_gate_tool_requests`,
    /// which the agent runs before every batch of *its own* model's tool calls.
    /// A coding-agent turn produces no such batch — the child runs its own loop
    /// and its tool calls arrive here over MCP instead — so that line never runs
    /// for a bridged turn, and until now nothing on this path ran in its place.
    ///
    /// What that left behind is a jail set by whatever ran **last** anywhere in
    /// the process. Both directions are wrong and neither is visible from the
    /// tool's error: an Auto-mode Codex turn following an Approve-mode chat has
    /// its writes to `/tmp` rejected as being outside the working directory (the
    /// exact false rejection the 2026-07-19 tool-errors audit found and the Auto
    /// relaxation exists to prevent), while an Approve-mode bridged turn
    /// following an Auto-mode chat writes wherever it likes with the jail down.
    /// In a fresh daemon that has only ever served a bridged provider the flag is
    /// still at its `false` initial value, so the first symptom users meet is the
    /// first of those two.
    ///
    /// The policy itself is not restated here — `Auto ⇒ relaxed` is read off the
    /// grant's own `mode`, the same field the inspectors below are handed, so the
    /// jail and the inspection can never disagree about which mode this call is
    /// running under. Sensitive-path writes stay gated by the `SensitiveOpsInspector`
    /// in that inspection pass either way.
    ///
    /// It runs before the inspectors rather than immediately before dispatch, for
    /// the same reason the agent's does: a refused call has still touched a global
    /// that the *next* call reads, and leaving that write until after a refusal
    /// would make the flag's value depend on whether the previous call was allowed.
    fn sync_path_jail(&self) {
        biorouter_mcp::set_path_jail_relaxed(self.mode == BioRouterMode::Auto);
    }

    /// Run one bridged tool call through the full gate stack.
    ///
    /// The inspector pass is the reason this is not a thin proxy onto
    /// `ExtensionManager`. `POST /agent/call_tool` is that thin proxy, and its own
    /// comment records what it costs: it "bypasses the agent loop and therefore
    /// every ToolInspector". A child agent's tool calls are model-initiated and
    /// must be inspected exactly like the parent model's.
    ///
    /// A call the permission inspector routes to `needs_approval` is **refused**
    /// rather than parked. The child is blocked on an HTTP response and there is
    /// no channel through which a human could answer it, so waiting would stall
    /// the turn until the timeout; refusing tells the child's model to ask the
    /// user in words. Refusing is also the fail-safe direction.
    ///
    /// BR-19's PreToolUse **rewrite** is honoured here, and the sequence below is
    /// `Agent::inspect_and_gate_tool_requests`' sequence rather than a shortened
    /// version of it — see [`Self::collect_hook_rewrites`] for why the second
    /// inspection pass is not optional.
    pub async fn call(&self, call: CallToolRequestParams) -> Result<CallToolResult, String> {
        self.sync_path_jail();

        let name = call.name.to_string();
        let mut requests = vec![ToolRequest {
            id: uuid::Uuid::new_v4().to_string(),
            tool_call: Ok(call),
            metadata: None,
            tool_meta: None,
        }];

        let mut inspections = self
            .inspections
            .inspect_tools(
                &requests,
                self.conversation.messages(),
                self.mode,
                &self.session,
            )
            .await
            .map_err(|e| format!("could not inspect `{name}`: {e}"))?;

        self.collect_hook_rewrites(&mut requests, &mut inspections)
            .await
            .map_err(|e| format!("could not re-inspect the rewritten `{name}`: {e}"))?;

        // No permission decision must never read as approval.
        let decision = self
            .inspections
            .process_inspection_results_with_permission_inspector(&requests, &inspections)
            .ok_or_else(|| format!("no permission decision was reached for `{name}`"))?;

        if !decision.denied.is_empty() {
            return Err(format!("`{name}` was denied by Biorouter's tool policy."));
        }
        if !decision.needs_approval.is_empty() {
            return Err(format!(
                "`{name}` needs a person's approval, and this turn has no way to ask for one. \
                 Tell the user what you wanted to run and why, and let them approve it."
            ));
        }

        // What runs is what was APPROVED, taken out of the verdict rather than
        // out of the request the child sent — the same way
        // `handle_approved_and_denied_tools` takes it on the agent's path. Reusing
        // the incoming `call` here would silently undo a hook rewrite that the
        // permission inspector had just judged, which is the whole defect this
        // sequence exists to close, restated one line later.
        let Some(approved) = decision.approved.into_iter().next() else {
            return Err(format!("`{name}` was not approved."));
        };
        let mut call = approved
            .tool_call
            .map_err(|e| format!("`{name}` was approved but is not a usable call: {e}"))?;
        self.apply_vault(&mut call);

        let result = self
            .extensions
            .dispatch_tool_call(
                &self.session.id,
                call,
                self.capability,
                self.dispatch_cancel_token(),
            )
            .await
            .map_err(|e| format!("`{name}` failed: {e}"))?;

        result
            .result
            .await
            .map_err(|e| format!("`{name}` failed: {e}"))
    }

    /// BRSDK encryption: resolve `{{vault:NAME}}` in the call's arguments.
    ///
    /// Placed exactly where `Agent::dispatch_tool_call` places
    /// `Agent::apply_vault` — on the leaf MCP-dispatch path, after the call has
    /// been judged and immediately before it runs — and the position is the whole
    /// design, not a detail. Earlier, the inspectors and the user's hooks would
    /// see the decrypted secret and a `SecurityInspector` reason or a hook's
    /// stdout could carry it out of the process. Later is not a place: the tool
    /// has already run.
    ///
    /// A bridged call without this ran with the literal placeholder string. That
    /// is worse than either working or failing, because nothing reports it: a
    /// placeholder is a perfectly valid string, so it goes out as an
    /// `Authorization: Bearer {{vault:API_KEY}}` header or into a config file and
    /// comes back as a 401 from a service Biorouter never names — for an app that
    /// works under every non-coding-agent provider.
    ///
    /// The grant carries a *snapshot* of the vault rather than a handle to the
    /// agent, so this is `&self` and synchronous, unlike the agent's version
    /// which has to take a mutex it shares with `set_vault`.
    ///
    /// The residual is the same one the agent's path has and is recorded there:
    /// a tool that echoes its arguments back in its *result* can still surface
    /// the secret. On this path the result reaches the child coding agent's model
    /// rather than Biorouter's own — a different context, the same exposure, and
    /// no worse, since a child that never gets the resolved call cannot do the
    /// job at all.
    fn apply_vault(&self, call: &mut CallToolRequestParams) {
        let Some(vault) = self.vault.as_ref() else {
            return;
        };
        if let Some(args) = call.arguments.as_mut() {
            vault.resolve_args(args);
        }
    }

    /// BR-19: apply whatever the PreToolUse hooks asked to rewrite, and re-inspect.
    ///
    /// The inspection pass above includes `HookInspector`, so a user's PreToolUse
    /// hooks have **already run** by the time this is called — their side effects
    /// happened, and a hook that returned an `updated_input` has staged that
    /// rewrite inside the `HooksManager`. Collecting it is a second, explicit
    /// step: `inspect_tools` returns inspection results, not arguments. Skipping
    /// that step is not a no-op, it is the worst of the three possible outcomes —
    /// the hook ran, the user believes their sandboxing/redaction/normalisation
    /// applied, and the untouched command executed anyway. (The agent's own path
    /// has always done this; the bridge simply never did, so a hook that behaved
    /// correctly in a normal chat silently stopped working the moment the user
    /// switched to `claude_code` or `codex`.)
    ///
    /// The re-inspection is the load-bearing half. The security and permission
    /// inspectors ran on the arguments the child's model produced, **not** on the
    /// rewritten ones, so dispatching a rewrite without re-judging it would turn
    /// any PreToolUse hook into a hole straight through them. Every inspector is
    /// re-run except the hook one, which is excluded for two reasons that both
    /// bite: re-running it would execute the user's hook commands a second time
    /// (side effects twice for one tool call), and it would let a rewrite trigger
    /// a further rewrite with no fixed point. The first pass's hook results are
    /// kept and everything else replaced, so the verdict is read off exactly one
    /// judgement of each kind.
    ///
    /// Deliberately NOT reproduced here: the agent also injects `rewrite_notice`
    /// into the model's context, so its model learns that what ran is not what it
    /// asked for. That channel does not exist on this path — the child's model
    /// lives in another process with its own context, and the only surface
    /// reaching it is the tool result itself. It stays as it is rather than being
    /// approximated, because a notice spliced into an arbitrary tool's result
    /// would arrive as part of the data for every caller that parses one. What
    /// the child gets is the honest execution of the rewritten call.
    async fn collect_hook_rewrites(
        &self,
        requests: &mut Vec<ToolRequest>,
        inspections: &mut Vec<crate::tool_inspection::InspectionResult>,
    ) -> anyhow::Result<()> {
        let rewrites = self.hooks.take_tool_input_rewrites(&self.session.id);
        if rewrites.is_empty() || crate::hooks::apply_tool_input_rewrites(requests, &rewrites) == 0
        {
            return Ok(());
        }

        let mut revalidated = self
            .inspections
            .inspect_tools_excluding(
                &[crate::hooks::inspector::HOOK_INSPECTOR_NAME],
                requests,
                self.conversation.messages(),
                self.mode,
                &self.session,
            )
            .await?;
        inspections
            .retain(|result| result.inspector_name == crate::hooks::inspector::HOOK_INSPECTOR_NAME);
        inspections.append(&mut revalidated);
        Ok(())
    }
}

/// Whether a provider, named by its registry id, drives a child that must call
/// back in over MCP.
///
/// A short list rather than a trait method: the `Provider` trait is implemented by
/// forty-odd modules and adding a method for a property two of them have would
/// make every other module state something irrelevant. The ids are the same
/// strings `pricing::blocks_fallback_pricing` keys on.
pub fn provider_uses_bridge(provider_name: &str) -> bool {
    matches!(provider_name, "claude_code" | "codex")
}

/// Live grants, keyed by nonce.
static GRANTS: LazyLock<RwLock<HashMap<String, Arc<BridgeGrant>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// The daemon's own base URL, published once it has bound a port.
///
/// `None` in a CLI process with no HTTP server, which is exactly the case where
/// the bridge must not be offered: there would be nothing for the child to
/// connect to. The providers treat absence as "run tool-less" rather than as an
/// error.
static BASE_URL: LazyLock<RwLock<Option<String>>> = LazyLock::new(|| RwLock::new(None));

/// Called by the server once it knows its bound address.
pub fn publish_base_url(base: impl Into<String>) {
    if let Ok(mut guard) = BASE_URL.write() {
        *guard = Some(base.into());
    }
}

pub fn base_url() -> Option<String> {
    BASE_URL.read().ok().and_then(|g| g.clone())
}

/// A live grant plus the URL to reach it. Dropping it revokes the grant, so a
/// panicking or early-returning turn cannot leave a capability behind.
pub struct BridgeLease {
    nonce: String,
    url: String,
}

impl BridgeLease {
    /// The URL to hand the child. Carries the nonce, which *is* the credential.
    pub fn url(&self) -> &str {
        &self.url
    }
}

impl Drop for BridgeLease {
    fn drop(&mut self) {
        if let Ok(mut guard) = GRANTS.write() {
            guard.remove(&self.nonce);
        }
    }
}

/// Register a grant and return its lease, or `None` when there is no HTTP server
/// to serve it.
pub fn issue(grant: BridgeGrant) -> Option<BridgeLease> {
    let base = base_url()?;
    // 32 hex characters from a v4 uuid: unguessable, and the whole credential.
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let url = format!("{}/tool_bridge/{nonce}", base.trim_end_matches('/'));
    GRANTS.write().ok()?.insert(nonce.clone(), Arc::new(grant));
    Some(BridgeLease { nonce, url })
}

/// Look up a grant by the nonce in the request path.
pub fn lookup(nonce: &str) -> Option<Arc<BridgeGrant>> {
    GRANTS.read().ok()?.get(nonce).cloned()
}

/// How many grants are live.
///
/// Diagnostic rather than test-facing: `GRANTS` is process-global, so a count is
/// only meaningful to an operator looking for a leak, never to a concurrent test.
pub fn live_grants() -> usize {
    GRANTS.read().map(|g| g.len()).unwrap_or(0)
}

tokio::task_local! {
    /// The URL of the bridge serving the turn currently on this task, if any.
    ///
    /// A task-local rather than an argument because the `Provider` trait has no
    /// session and no agent in scope — `complete_with_model` receives only a
    /// system prompt, messages and tools. The agent sets this around the provider
    /// call, and the coding-agent providers read it to build the child's MCP
    /// configuration.
    ///
    /// ⚠ Read it at **construction** time, never from inside a stream's poll. The
    /// scope wraps the awaited call that builds the response, not the consumption
    /// of what that call returns — so a `stream()` implementation may read this
    /// (it runs inside the scope, and is where the child is spawned), while a
    /// poll of the returned stream may not: by then the scope is gone and the
    /// task-local reads `None`.
    ///
    /// The *lease* is not the constraint here, and an earlier version of this note
    /// wrongly implied it was: `Agent::reply` binds it before the scope and it
    /// lives to the end of that loop iteration, which outlasts stream
    /// consumption. What a streaming implementation has to do is capture the URL
    /// up front, not thread the lease differently.
    pub static ACTIVE_BRIDGE_URL: Option<String>;
}

/// The bridge URL for the current turn, if the agent established one.
pub fn active_bridge_url() -> Option<String> {
    ACTIVE_BRIDGE_URL.try_with(|url| url.clone()).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lease revokes its grant when dropped. A grant that outlived its turn
    /// would be a live capability onto a session's tools with nothing owning it.
    ///
    /// Asserted by membership rather than by counting: `GRANTS` is process-global
    /// and these tests run concurrently, so a count would be measuring the other
    /// test's grants as well.
    /// Only the two coding-agent providers get a bridge. Issuing one for every
    /// provider would leave a live capability on every turn in the process.
    #[test]
    fn only_the_coding_agent_providers_use_the_bridge() {
        assert!(provider_uses_bridge("claude_code"));
        assert!(provider_uses_bridge("codex"));
        for other in [
            "anthropic",
            "openai",
            "llamacpp",
            "ollama",
            "versa_azure",
            "",
        ] {
            assert!(
                !provider_uses_bridge(other),
                "{other} receives its tools in the request and needs no grant"
            );
        }
    }

    #[tokio::test]
    async fn dropping_a_lease_revokes_the_grant() {
        publish_base_url("http://127.0.0.1:65535");
        let lease = issue(dummy_grant()).expect("a base URL is published");

        let nonce = lease
            .url()
            .rsplit('/')
            .next()
            .expect("the url ends in the nonce")
            .to_string();
        assert!(lookup(&nonce).is_some(), "the grant should be reachable");

        drop(lease);
        assert!(
            lookup(&nonce).is_none(),
            "the grant must not outlive its lease"
        );
    }

    /// The nonce is the credential, so it must be long, unguessable and unique per
    /// lease.
    #[tokio::test]
    async fn each_lease_gets_its_own_unguessable_nonce() {
        publish_base_url("http://127.0.0.1:65535");
        let a = issue(dummy_grant()).unwrap();
        let b = issue(dummy_grant()).unwrap();
        assert_ne!(a.url(), b.url());
        for lease in [&a, &b] {
            let nonce = lease.url().rsplit('/').next().unwrap();
            assert_eq!(nonce.len(), 32, "a short nonce is a guessable capability");
            assert!(nonce.chars().all(|c| c.is_ascii_hexdigit()));
        }
        assert!(a.url().contains("/tool_bridge/"));
    }

    /// Outside a turn there is no bridge, and asking must not panic — the
    /// task-local is simply unset in every other task, including the HTTP handler.
    #[test]
    fn asking_for_the_bridge_outside_a_turn_is_none() {
        assert!(active_bridge_url().is_none());
    }

    #[tokio::test]
    async fn the_url_is_visible_inside_the_scope_and_not_outside() {
        let scoped = ACTIVE_BRIDGE_URL
            .scope(
                Some("http://127.0.0.1:1/tool_bridge/abc".to_string()),
                async { active_bridge_url() },
            )
            .await;
        assert_eq!(
            scoped.as_deref(),
            Some("http://127.0.0.1:1/tool_bridge/abc")
        );
        assert!(active_bridge_url().is_none(), "the scope must not leak");
    }

    /// The bridge fails CLOSED. With no permission inspector configured there is no
    /// decision to read, and the only safe reading of "nothing decided" is refusal —
    /// the alternative is a bridged child agent executing a tool that no inspector
    /// ever looked at.
    ///
    /// Worth pinning separately from the happy path because the failure is silent:
    /// an `unwrap_or_default()` here, or an `if denied { .. }` with no final
    /// `else` refusal, would turn a misconfigured stack into an open door and every
    /// other test in this file would still pass.
    #[tokio::test]
    async fn a_grant_with_no_permission_inspector_refuses_rather_than_allows() {
        // `call()` writes the process-global path jail on its way in, so this
        // shares the lock with the test that asserts on that flag even though it
        // asserts nothing about it itself.
        let _guard = path_jail_lock();
        let grant = dummy_grant();
        let call = CallToolRequestParams {
            name: "developer__shell".to_string().into(),
            arguments: None,
            meta: None,
            task: None,
        };

        let refusal = grant
            .call(call)
            .await
            .expect_err("an uninspected tool call must not be executed");
        assert!(
            refusal.contains("no permission decision"),
            "the refusal should name the missing decision, got: {refusal}"
        );
    }

    /// Cancelling the turn must cancel a tool the bridged child started.
    ///
    /// This is the whole of issue #72's kill path, `AppState::cancel_turn` and
    /// `TurnGuard` at once: all three reach a running tool through the token the
    /// agent threads down, and `ExtensionManager::dispatch_tool_call` takes that
    /// token by value with no other channel back to the turn. A grant that made
    /// its own token would satisfy the type system and leave every one of those
    /// mechanisms with nothing to pull — the user presses stop, the child dies,
    /// and the `developer__shell` it launched keeps running detached.
    ///
    /// Asserted through [`BridgeGrant::dispatch_cancel_token`] because that is
    /// the exact value the dispatch site passes; a test that only cancelled a
    /// token it had built itself would pass against a grant that ignores it.
    #[tokio::test]
    async fn the_turns_cancel_reaches_a_bridged_tool() {
        let turn = CancellationToken::new();
        let grant = grant_cancelled_by(Some(turn.clone()));

        assert!(
            !grant.dispatch_cancel_token().is_cancelled(),
            "nothing has cancelled the turn yet"
        );

        turn.cancel();

        assert!(
            grant.dispatch_cancel_token().is_cancelled(),
            "a bridged tool must be reachable by the turn's own cancel; a token \
             constructed at the dispatch site is held by nobody and never fires"
        );
    }

    /// A turn genuinely without a token (a workflow step, a test harness) must
    /// still dispatch. "No token" means "never cancelled", exactly as it does in
    /// `Agent::dispatch_tool_call`, which resolves the same `Option` the same
    /// way — not an error, and not a refusal to run the tool.
    #[tokio::test]
    async fn a_turn_with_no_token_still_dispatches() {
        let grant = grant_cancelled_by(None);
        assert!(!grant.dispatch_cancel_token().is_cancelled());
    }

    /// A bridged call jails `text_editor` by ITS OWN session's mode.
    ///
    /// The flag is a process-global atomic whose one production setter sits at the
    /// top of the agent's own inspection batch, and a coding-agent turn never
    /// reaches that line — the child runs its own loop. So before this, a bridged
    /// call ran under whatever the previous session in the process had left, in
    /// either direction: an Auto-mode Codex turn rejecting a legitimate `/tmp`
    /// write, or an Approve-mode one writing anywhere with the jail down.
    ///
    /// Driven through `call()` rather than through `sync_path_jail()` directly,
    /// because the defect was never in the policy — it was that nothing on this
    /// path ran it. A test that called the helper would have passed against the
    /// broken code the moment the helper existed.
    ///
    /// The two orders are both asserted from a *hostile* starting value, so the
    /// test cannot pass by the flag already happening to hold the answer.
    #[tokio::test]
    async fn a_bridged_call_sets_the_path_jail_from_its_own_mode() {
        // `PATH_JAIL_RELAXED` is process-global, so the two grants below have to
        // take turns; a `tokio::test` gives each its own runtime but not its own
        // process.
        let _guard = path_jail_lock();

        for (mode, expected) in [
            (BioRouterMode::Auto, true),
            (BioRouterMode::Approve, false),
            (BioRouterMode::Chat, false),
        ] {
            // Leave the flag holding the opposite of the answer, the way a
            // previous session of the other mode would have.
            biorouter_mcp::set_path_jail_relaxed(!expected);

            let grant = grant_in_mode(mode);
            // The call itself is refused (no permission inspector is configured),
            // which is deliberate: the jail must be pointed at this session before
            // any decision is taken, so that a refusal cannot leave the next call
            // reading a mode that is not its own.
            let _ = grant
                .call(CallToolRequestParams {
                    name: "developer__text_editor".to_string().into(),
                    arguments: None,
                    meta: None,
                    task: None,
                })
                .await;

            assert_eq!(
                biorouter_mcp::path_jail_relaxed(),
                expected,
                "a bridged call in {mode:?} must set the jail itself; leaving it \
                 unset means the last session in the process decides whether this \
                 one may write outside its working directory"
            );
        }

        biorouter_mcp::set_path_jail_relaxed(false);
    }

    /// BR-19 end-to-end: a PreToolUse rewrite decides what a bridged call runs.
    ///
    /// Deliberately driven through a real in-process extension and a real hook
    /// command rather than by staging a rewrite by hand, because the defect was
    /// never in `apply_tool_input_rewrites` — that function was correct and
    /// tested. The defect was that the bridge ran the user's hooks (side effects
    /// and all), let them stage a rewrite, and then dispatched the arguments the
    /// hook had asked to replace. Only a test that watches what actually
    /// *executed* can tell those two apart: the request id the rewrite is keyed
    /// on is a uuid minted inside `call()`, so a hand-staged rewrite could never
    /// have matched it anyway.
    ///
    /// The hook rewrites a query for chromosome 7 into one for chromosome 17, so
    /// the returned rows name which arguments reached SQLite.
    #[tokio::test]
    async fn a_pretooluse_rewrite_decides_what_a_bridged_call_runs() {
        let _guard = path_jail_lock();
        let fixture = GeneFixture::new().await;

        let hooks = hooks_rewriting_query_to("17");
        let grant = fixture.grant(inspections_with(&hooks, false), Arc::clone(&hooks));

        let result = grant
            .call(fixture.query_for("7"))
            .await
            .expect("the rewritten query is a valid one");

        let text = serde_json::to_string(&result).expect("a serialisable result");
        assert!(
            text.contains("TP53"),
            "the hook rewrote the query to chromosome 17, so its row is what must \
             come back; a bridge that drops the rewrite runs the model's original \
             query and the user's hook silently did nothing. got: {text}"
        );
        assert!(
            !text.contains("CFTR"),
            "the model's original chromosome-7 query must NOT be what ran: {text}"
        );
    }

    /// A rewrite is re-judged, not waved through.
    ///
    /// The security and permission inspectors ran on the arguments the child's
    /// model produced. If a rewrite were dispatched without a second pass, every
    /// PreToolUse hook would be a hole straight through them — a user's own hook
    /// is the obvious case, but the hook config is also project-level and
    /// managed-policy-supplied, so "the user wrote it" is not a safety argument.
    ///
    /// Here the hook rewrites a harmless `ls` into a catastrophic `rm -rf /`,
    /// which `SecurityInspector`'s non-bypassable floor denies. Without the
    /// re-inspection the floor only ever sees the `ls`, the call is approved, and
    /// the refusal that comes back (if any) is a dispatch failure rather than a
    /// policy denial — which is why this asserts on the *reason*.
    #[tokio::test]
    async fn a_rewritten_call_is_re_judged_by_the_security_floor() {
        let _guard = path_jail_lock();
        let fixture = GeneFixture::new().await;

        let hooks = hooks_rewriting_shell_to("rm -rf /");
        let grant = fixture.grant(inspections_with(&hooks, true), Arc::clone(&hooks));

        let refusal = grant
            .call(CallToolRequestParams {
                name: "developer__shell".to_string().into(),
                arguments: Some(
                    serde_json::json!({ "command": "ls" })
                        .as_object()
                        .expect("an object")
                        .clone(),
                ),
                meta: None,
                task: None,
            })
            .await
            .expect_err("a rewritten catastrophic command must not run");

        assert!(
            refusal.contains("denied by Biorouter's tool policy"),
            "the rewritten command must be judged by the inspectors that only saw \
             the original; got: {refusal}"
        );
    }

    /// BRSDK: a `{{vault:NAME}}` in a bridged call is resolved before it runs.
    ///
    /// The placeholder is put where the *answer* depends on it — inside the
    /// query's `WHERE` — because that is the only way to tell resolution from
    /// non-resolution here. An unresolved placeholder is not an error and raises
    /// nothing: it is a valid string that matches no chromosome, so the call
    /// succeeds and returns nothing, which in production is a header that goes
    /// out reading `Bearer {{vault:API_KEY}}` and a 401 from a service Biorouter
    /// never names.
    #[tokio::test]
    async fn a_bridged_call_resolves_its_vault_references() {
        let _guard = path_jail_lock();
        let fixture = GeneFixture::new().await;

        let hooks = no_hooks();
        let vault = Arc::new(crate::agents::vault_refs::VaultRefs::new(HashMap::from([
            ("CHROM".to_string(), "17".to_string()),
        ])));
        let grant = fixture.grant_with_vault(inspections_with(&hooks, false), hooks, Some(vault));

        let result = grant
            .call(fixture.query_for("{{vault:CHROM}}"))
            .await
            .expect("the resolved query is a valid one");

        let text = serde_json::to_string(&result).expect("a serialisable result");
        assert!(
            text.contains("TP53"),
            "`{{{{vault:CHROM}}}}` must have become `17` before the query ran; an \
             unresolved placeholder matches nothing and fails silently. got: {text}"
        );
    }

    /// The same call with no vault installed leaves the placeholder alone.
    ///
    /// Not symmetry for its own sake: it pins that resolution is the vault's doing
    /// and not something the SQL layer or the argument plumbing does on its own,
    /// which is what makes the assertion above evidence of anything. Normal
    /// (non-BRSDK) sessions are this case, and they are the overwhelming majority.
    #[tokio::test]
    async fn without_a_vault_a_placeholder_is_left_as_written() {
        let _guard = path_jail_lock();
        let fixture = GeneFixture::new().await;

        let hooks = no_hooks();
        let grant = fixture.grant(inspections_with(&hooks, false), hooks);

        let result = grant
            .call(fixture.query_for("{{vault:CHROM}}"))
            .await
            .expect("a query with no matching rows is still a valid query");

        let text = serde_json::to_string(&result).expect("a serialisable result");
        assert!(
            !text.contains("TP53") && !text.contains("CFTR"),
            "with no vault the placeholder is a literal that matches no row: {text}"
        );
    }

    /// A sqlite database with two rows on two different chromosomes, plus the
    /// `datasql` extension serving it in-process. Two rows on distinguishable
    /// keys is the whole point: "which arguments ran" has to be readable off the
    /// output, not inferred.
    struct GeneFixture {
        _dir: tempfile::TempDir,
        extensions: Arc<ExtensionManager>,
    }

    impl GeneFixture {
        async fn new() -> Self {
            use biorouter_mcp::datasql::server::DataSqlServer;
            use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

            let dir = tempfile::tempdir().expect("a temp dir");
            let db_path = dir.path().join("cohort.db");
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(
                    SqliteConnectOptions::new()
                        .filename(&db_path)
                        .create_if_missing(true),
                )
                .await
                .expect("a sqlite file");
            sqlx::query("CREATE TABLE genes (symbol TEXT, chrom TEXT)")
                .execute(&pool)
                .await
                .expect("a table");
            sqlx::query("INSERT INTO genes VALUES ('CFTR','7'), ('TP53','17')")
                .execute(&pool)
                .await
                .expect("two rows");
            pool.close().await;

            let extensions = Arc::new(ExtensionManager::new_without_provider(
                dir.path().to_path_buf(),
            ));
            let mut sources = HashMap::new();
            sources.insert("cohort".to_string(), db_path);
            extensions
                .add_inprocess_server("datasql", DataSqlServer::new(sources))
                .await
                .expect("the datasql extension loads in-process");

            Self {
                _dir: dir,
                extensions,
            }
        }

        fn query_for(&self, chrom: &str) -> CallToolRequestParams {
            CallToolRequestParams {
                name: "datasql__data_query".to_string().into(),
                arguments: Some(query_args(chrom).as_object().expect("an object").clone()),
                meta: None,
                task: None,
            }
        }

        fn grant(
            &self,
            inspections: ToolInspectionManager,
            hooks: Arc<crate::hooks::HooksManager>,
        ) -> BridgeGrant {
            self.grant_with_vault(inspections, hooks, None)
        }

        fn grant_with_vault(
            &self,
            inspections: ToolInspectionManager,
            hooks: Arc<crate::hooks::HooksManager>,
            vault: Option<Arc<crate::agents::vault_refs::VaultRefs>>,
        ) -> BridgeGrant {
            BridgeGrant::new(
                Session::default(),
                // Auto, so the permission inspector approves and the test measures
                // the rewrite rather than the approval flow.
                BioRouterMode::Auto,
                Arc::clone(&self.extensions),
                Arc::new(inspections),
                CallCapability::public_enforced(),
                vec![],
                Conversation::new_unvalidated(vec![]),
                None,
                hooks,
                vault,
            )
        }
    }

    fn query_args(chrom: &str) -> serde_json::Value {
        serde_json::json!({
            "source": "cohort",
            "sql": format!("SELECT symbol FROM genes WHERE chrom='{chrom}'"),
        })
    }

    /// The two inspectors a bridged call's verdict is actually read off, plus the
    /// security floor when the test needs it. Not the agent's full stack: every
    /// other inspector there is inert for these tools and would only add ways for
    /// the test to fail for a reason it is not about.
    fn inspections_with(
        hooks: &Arc<crate::hooks::HooksManager>,
        with_security: bool,
    ) -> ToolInspectionManager {
        let mut manager = ToolInspectionManager::new();
        if with_security {
            manager.add_inspector(Box::new(
                crate::security::security_inspector::SecurityInspector::new(),
            ));
        }
        manager.add_inspector(Box::new(
            crate::permission::permission_inspector::PermissionInspector::new(
                Arc::new(crate::permission::tool_risk::ToolRiskRegistry::new()),
                Arc::new(crate::config::permission::PermissionManager::new(
                    std::env::temp_dir().join(format!("br-bridge-perms-{}", uuid::Uuid::new_v4())),
                )),
                Arc::new(crate::managed::ManagedPolicy::empty()),
                Arc::new(tokio::sync::Mutex::new(None)),
            ),
        ));
        manager.add_inspector(Box::new(crate::hooks::HookInspector::new(Arc::clone(
            hooks,
        ))));
        manager
    }

    fn hooks_rewriting_query_to(chrom: &str) -> Arc<crate::hooks::HooksManager> {
        hooks_rewriting(
            "datasql__data_query",
            &serde_json::json!({ "updatedInput": query_args(chrom) }),
        )
    }

    fn hooks_rewriting_shell_to(command: &str) -> Arc<crate::hooks::HooksManager> {
        hooks_rewriting(
            "developer__shell",
            &serde_json::json!({ "updatedInput": { "command": command } }),
        )
    }

    /// A `HooksManager` holding one PreToolUse hook that prints a
    /// `hookSpecificOutput` rewrite for `matcher`.
    ///
    /// A real shell command rather than a stub, because the staging happens
    /// inside the manager as a side effect of the hook *running*; a fake that
    /// staged directly would test the assertion rather than the path.
    fn hooks_rewriting(
        matcher: &str,
        hook_specific_output: &serde_json::Value,
    ) -> Arc<crate::hooks::HooksManager> {
        let payload = serde_json::json!({ "hookSpecificOutput": hook_specific_output }).to_string();
        // Single-quoted for `sh -c`, with any embedded quote escaped the usual
        // way. The JSON above contains none, but a future edit to the fixtures
        // should not become a mysterious hook failure.
        let quoted = payload.replace('\'', "'\"'\"'");
        let yaml = format!(
            "PreToolUse:\n  - matcher: {}\n    hooks:\n      - type: command\n        command: {}\n",
            serde_json::to_string(matcher).expect("a json string"),
            serde_json::to_string(&format!("echo '{quoted}'")).expect("a json string"),
        );
        let config = serde_yaml::from_str(&yaml).expect("the hook config parses");
        Arc::new(crate::hooks::HooksManager::with_config(
            config,
            false,
            Arc::new(tokio::sync::Mutex::new(None)),
        ))
    }

    /// Serializes every test in this module that touches the process-global path
    /// jail. Poison is recovered rather than propagated: the guarded value is a
    /// `()` and a panicking test leaves no half-updated invariant behind, so
    /// refusing to run the next test would only convert one failure into two.
    fn path_jail_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn dummy_grant() -> BridgeGrant {
        grant_cancelled_by(None)
    }

    fn grant_in_mode(mode: BioRouterMode) -> BridgeGrant {
        BridgeGrant::new(
            Session::default(),
            mode,
            Arc::new(ExtensionManager::new(
                Arc::new(tokio::sync::Mutex::new(None)),
                Arc::new(crate::session::SessionManager::instance()),
            )),
            Arc::new(ToolInspectionManager::new()),
            CallCapability::public_enforced(),
            vec![],
            Conversation::new_unvalidated(vec![]),
            None,
            no_hooks(),
            None,
        )
    }

    /// A manager with no hooks configured, for the tests that are not about
    /// them. Its `take_tool_input_rewrites` is empty, so `call()` takes the
    /// early return in `collect_hook_rewrites` and nothing is re-inspected.
    fn no_hooks() -> Arc<crate::hooks::HooksManager> {
        Arc::new(crate::hooks::HooksManager::with_config(
            Default::default(),
            false,
            Arc::new(tokio::sync::Mutex::new(None)),
        ))
    }

    fn grant_cancelled_by(cancel: Option<CancellationToken>) -> BridgeGrant {
        BridgeGrant::new(
            Session::default(),
            BioRouterMode::Auto,
            Arc::new(ExtensionManager::new(
                Arc::new(tokio::sync::Mutex::new(None)),
                Arc::new(crate::session::SessionManager::instance()),
            )),
            Arc::new(ToolInspectionManager::new()),
            CallCapability::public_enforced(),
            vec![],
            Conversation::new_unvalidated(vec![]),
            cancel,
            no_hooks(),
            None,
        )
    }
}
