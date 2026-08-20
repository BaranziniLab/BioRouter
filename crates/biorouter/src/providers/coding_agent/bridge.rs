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
    pub async fn call(&self, call: CallToolRequestParams) -> Result<CallToolResult, String> {
        let name = call.name.to_string();
        let requests = vec![ToolRequest {
            id: uuid::Uuid::new_v4().to_string(),
            tool_call: Ok(call.clone()),
            metadata: None,
            tool_meta: None,
        }];

        let inspections = self
            .inspections
            .inspect_tools(
                &requests,
                self.conversation.messages(),
                self.mode,
                &self.session,
            )
            .await
            .map_err(|e| format!("could not inspect `{name}`: {e}"))?;

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
        if decision.approved.is_empty() {
            return Err(format!("`{name}` was not approved."));
        }

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

    fn dummy_grant() -> BridgeGrant {
        grant_cancelled_by(None)
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
        )
    }
}
