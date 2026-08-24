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
use std::time::Duration;

use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use tokio_util::sync::CancellationToken;

use crate::agents::extension_manager::ExtensionManager;
use crate::config::BioRouterMode;
use crate::conversation::message::ToolRequest;
use crate::conversation::Conversation;
use crate::pending_user_action::{
    PendingUserActions, ToolApprovalRequest, UserActionOutcome, UserActionRequest,
};
use crate::permission::tool_risk::ToolRiskRegistry;
use crate::privacy::CallCapability;
use crate::session::session_manager::Session;
use crate::tool_inspection::ToolInspectionManager;

/// What a bridged `tools/call` actually runs, once the gate stack has cleared it.
///
/// An abstraction rather than a hardcoded `ExtensionManager` because the bridge
/// is not only the chat loop's (#109). A knowledge macro, a scheduled workflow or
/// any other bounded sub-agent has its own small tool surface with its own
/// dispatcher — the ingest macro's `KbToolDispatch` carries the git transaction
/// every write in the run must land on — and those tools are not in any
/// `ExtensionManager` at all.
///
/// Before this, the only way to give a child agent tools was the session's whole
/// extension surface, so a macro running under `claude_code` or `codex` was
/// handed a `tools` argument its provider discarded. It then narrated the calls
/// as prose, invented its own results to continue against, and wrote nothing —
/// after a full model run. The UI's answer was a provider denylist.
///
/// Whatever implements this, everything above it is unchanged: the inspectors,
/// the permission decision, the privacy capability, the vault, the hooks and the
/// approval round trip all run first, exactly as they do for a chat turn. This
/// decides only *where the call lands*, never *whether it may*.
#[async_trait::async_trait]
pub trait BridgeToolDispatch: Send + Sync {
    async fn dispatch(
        &self,
        session_id: &str,
        call: CallToolRequestParams,
        capability: CallCapability,
        cancel: CancellationToken,
    ) -> Result<CallToolResult, String>;
}

#[async_trait::async_trait]
impl BridgeToolDispatch for ExtensionManager {
    async fn dispatch(
        &self,
        session_id: &str,
        call: CallToolRequestParams,
        capability: CallCapability,
        cancel: CancellationToken,
    ) -> Result<CallToolResult, String> {
        let name = call.name.to_string();
        let result = self
            .dispatch_tool_call(session_id, call, capability, cancel)
            .await
            .map_err(|e| format!("`{name}` failed: {e}"))?;
        result
            .result
            .await
            .map_err(|e| format!("`{name}` failed: {e}"))
    }
}

/// Everything one turn's bridge needs to serve `tools/list` and `tools/call`.
///
/// Deliberately a snapshot rather than a handle back to the `Agent`: the provider
/// is called from inside the agent's own stack and cannot hold a reference to it,
/// and a grant that outlived its turn would be a capability with no owner.
pub struct BridgeGrant {
    session: Session,
    mode: BioRouterMode,
    dispatcher: Arc<dyn BridgeToolDispatch>,
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
    /// BR-63's risk grades, so an approval card raised from here says *how*
    /// dangerous the call is — the same sentence the agent's own card says.
    ///
    /// A card that differed from the agent's would be a second, subtly worse
    /// dialog for the identical decision, and the user would have no way to know
    /// which one they were looking at.
    tool_risks: Arc<ToolRiskRegistry>,
    /// This grant's nonce, stamped by [`issue`] once it has minted one.
    ///
    /// Empty until then, and it must be: the nonce is the URL's credential and
    /// cannot exist before the grant it names. It is here so a parked approval
    /// can be *owned* by the turn — [`BridgeLease::drop`] cancels by owner, which
    /// is what stops a panicking turn leaving a child blocked on an HTTP response
    /// nobody will ever answer.
    nonce: String,
}

/// The per-call MCP deadline Biorouter asks each child CLI to apply (#110).
///
/// Both CLIs apply a hard per-call wall clock and abandon the request when it
/// elapses; the default is far shorter than Biorouter's own long-running tools
/// need. Issue #110 measured Claude Code's at ~60 s against a `workspace_watch`
/// whose schema advertises waits of up to 600 s — every one of which died at 60
/// with "The operation timed out", a transport failure the model may retry
/// rather than an answer it can act on.
///
/// So the deadline is configured rather than discovered. Both CLIs expose the
/// knob per MCP server, which is what makes this reliable rather than a hope:
///
/// | CLI | Where |
/// | --- | --- |
/// | Claude Code | `timeout` (**milliseconds**) in the `--mcp-config` server entry. Its own help calls it a "hard wall-clock limit per call; progress notifications do not extend it". |
/// | Codex | `mcp_servers.<name>.tool_timeout_sec` (**seconds**) in the `thread/start` config override. |
///
/// Deliberately **above** [`child_tool_call_budget`] rather than equal to it:
/// the budget is what a call may spend, and the difference is the room the
/// answer itself needs on the wire.
pub const CHILD_TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(660);

/// How long a bridged `tools/call` may actually spend before it must answer.
///
/// Anything on this path that waits must fit *inside* this and answer with a
/// real result, rather than letting the deadline pass and hoping. A tool that
/// runs past it does not get more time — it gets an abandoned request, and the
/// model reads that as a broken server.
pub fn child_tool_call_budget() -> Duration {
    Duration::from_secs(600)
}

/// The longest a bridged call may park on a human before it must answer the
/// child anyway.
///
/// Bounded by [`child_tool_call_budget`] and NOT by
/// `BIOROUTER_CONFIRMATION_TIMEOUT_SECS`'s hour: the agent's own prompt can wait
/// an hour because nothing is holding a socket open, and this one cannot,
/// because a child CLI is. Waiting past the budget does not buy the user more
/// time — it converts a card they could still answer into a transport error the
/// model reads as a broken server.
fn approval_ttl() -> Duration {
    child_tool_call_budget().saturating_sub(Duration::from_secs(30))
}

tokio::task_local! {
    /// How long the bridged tool call running on this task may take (#110).
    ///
    /// A task-local because the tools that need it are ordinary MCP handlers
    /// with no idea what is calling them — `workspace_watch` accepts a
    /// `timeout_s` up to 600 and must clamp it to what the *transport* allows,
    /// and it cannot be handed that through a schema every other caller shares.
    ///
    /// Absent means "not a bridged call": an ordinary agent turn holds nothing
    /// open, so nothing needs clamping.
    static BRIDGED_CALL_BUDGET: Duration;
}

/// The wall clock the bridged tool call on this task must answer within, or
/// `None` when this is not a bridged call.
///
/// A tool that waits should clamp to this and return a **partial result** at the
/// deadline. Running past it is not an option that trades latency for
/// completeness: the child abandons the request and the model is told the
/// operation timed out, which loses the partial answer as well as the wait.
pub fn bridged_call_budget() -> Option<Duration> {
    BRIDGED_CALL_BUDGET.try_with(|d| *d).ok()
}

/// Run `f` as though it were a bridged tool call with `budget` left.
///
/// For the tools that clamp to the budget: `workspace_watch`'s arithmetic is
/// what #110 is about, and exercising it through a real child CLI would make a
/// unit test depend on a subscription. The production scope is
/// [`BridgeGrant::call`] and there is no other.
pub async fn with_call_budget_for_test<F: std::future::Future>(
    budget: Duration,
    f: F,
) -> F::Output {
    BRIDGED_CALL_BUDGET.scope(budget, f).await
}

impl BridgeGrant {
    /// Eleven arguments, and grouping them would make this worse rather than
    /// tidier.
    ///
    /// Each one is a distinct thing the agent has to remember to hand over, and
    /// four of them (the cancel token, the hooks manager, the vault, the risk
    /// registry) were discovered missing precisely by reading this list against what
    /// `Agent::dispatch_tool_call` does. A `BridgeContext` wrapper would hide the
    /// list behind a name and move the omission one file further from the reader
    /// without removing a single field; the flat call site is what makes the
    /// audit possible. The pattern (an `allow` plus a reason) is the one used at
    /// fourteen other sites in this workspace, `Agent::inspect_and_gate_tool_requests`
    /// among them.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session: Session,
        mode: BioRouterMode,
        dispatcher: Arc<dyn BridgeToolDispatch>,
        inspections: Arc<ToolInspectionManager>,
        capability: CallCapability,
        tools: Vec<Tool>,
        conversation: Conversation,
        cancel: Option<CancellationToken>,
        hooks: Arc<crate::hooks::HooksManager>,
        vault: Option<Arc<crate::agents::vault_refs::VaultRefs>>,
        tool_risks: Arc<ToolRiskRegistry>,
    ) -> Self {
        Self {
            session,
            mode,
            dispatcher,
            inspections,
            capability,
            tools,
            conversation,
            cancel,
            hooks,
            vault,
            tool_risks,
            nonce: String::new(),
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
    /// value written is this call's own mode and not a second opinion about it.
    /// Sensitive-path writes stay gated by the `SensitiveOpsInspector` in that
    /// inspection pass either way.
    ///
    /// It runs before the inspectors rather than immediately before dispatch, for
    /// the same reason the agent's does: a refused call has still touched a global
    /// that the *next* call reads, and leaving that write until after a refusal
    /// would make the flag's value depend on whether the previous call was allowed.
    ///
    /// # What this does NOT establish
    ///
    /// ⚠ **Correct at the instant it is written, not for the duration of the
    /// call.** An earlier version of this note claimed the jail and the
    /// inspection "can never disagree about which mode this call is running
    /// under", and that is false: the flag is one process-global atomic shared by
    /// every session, and between this write and the dispatch below the call
    /// awaits through `inspect_tools` — which executes the user's PreToolUse
    /// hooks as real `sh -c` commands — and through `collect_hook_rewrites`. Any
    /// concurrent writer inside that window (another bridged call in another
    /// session, or `Agent::inspect_and_gate_tool_requests` on an ordinary turn)
    /// flips it, and an Approve-mode session's `text_editor` write can still run
    /// with the jail down.
    ///
    /// The agent's own path has exactly the same window, so this is a residual of
    /// the flag's design rather than something the bridge introduced — but the
    /// bridge does make it *bigger*, and honesty about that is the point of this
    /// paragraph: the agent writes once per batch of the model's tool calls,
    /// while this writes once per bridged tool call, which for co-resident
    /// sessions is a materially higher collision rate. Closing it properly means
    /// making the jail per-call state rather than a process global — the
    /// `CallCapability` treatment, applied to a second global — and that is a
    /// change to `biorouter_mcp`'s developer server, not to this file. Not doing
    /// it here is deliberate; not writing it down was the defect.
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
    ///
    /// The request id is minted **here**, above the work, rather than inside it,
    /// because it is this call's name in the `HooksManager`'s per-session staging
    /// buffer and two separate steps need it: taking this call's rewrite without
    /// taking a concurrent sibling's, and clearing this call's staged context
    /// afterwards. Every exit from [`Self::dispatch_one`] goes through the drain
    /// below, refusals included — a call that was denied still ran the user's
    /// PreToolUse hooks and still staged whatever they returned.
    pub async fn call(&self, call: CallToolRequestParams) -> Result<CallToolResult, String> {
        let request_id = uuid::Uuid::new_v4().to_string();
        // #110: publish the transport's budget for the duration of the call, so
        // a tool that waits can clamp to it instead of running past the child's
        // deadline and losing its partial answer to an abandoned request.
        let outcome = BRIDGED_CALL_BUDGET
            .scope(
                child_tool_call_budget(),
                self.dispatch_one(request_id.clone(), call),
            )
            .await;
        self.discard_staged_hook_context(&request_id);
        outcome
    }

    /// The body of one bridged call, from the path jail to the tool's result.
    ///
    /// Split out of [`Self::call`] only so that the staged-context drain there
    /// cannot be skipped by any of this function's several early returns.
    async fn dispatch_one(
        &self,
        request_id: String,
        call: CallToolRequestParams,
    ) -> Result<CallToolResult, String> {
        self.sync_path_jail();

        let name = call.name.to_string();
        let mut requests = vec![ToolRequest {
            id: request_id,
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
        // What runs is what was APPROVED, taken out of the verdict rather than
        // out of the request the child sent — the same way
        // `handle_approved_and_denied_tools` takes it on the agent's path. Reusing
        // the incoming `call` here would silently undo a hook rewrite that the
        // permission inspector had just judged, which is the whole defect this
        // sequence exists to close, restated one line later.
        //
        // #107: a call the *user* approves is taken out of `needs_approval` for
        // the identical reason. That entry is the same post-rewrite request the
        // inspectors judged and the card showed them, so it — not the child's
        // original arguments — is what their "allow" was an answer to.
        let approved = match decision.approved.into_iter().next() {
            Some(approved) => approved,
            None => {
                let Some(pending) = decision.needs_approval.into_iter().next() else {
                    return Err(format!("`{name}` was not approved."));
                };
                self.await_approval(&name, &pending).await?;
                pending
            }
        };
        let mut call = approved
            .tool_call
            .map_err(|e| format!("`{name}` was approved but is not a usable call: {e}"))?;
        self.apply_vault(&mut call);

        self.dispatcher
            .dispatch(
                &self.session.id,
                call,
                self.capability,
                self.dispatch_cancel_token(),
            )
            .await
    }

    /// Put a `needs_approval` call to a person, and park until they answer.
    ///
    /// # What this replaced, and why the old answer was not merely unhelpful
    ///
    /// Until #107 this returned a refusal telling the child's model to "tell the
    /// user what you wanted to run and why, and let them approve it". The model
    /// did exactly that, and the sentence was false in a way nothing on screen
    /// revealed: no request id had been minted, so there was no dialog, nothing
    /// for a client to post to, and no way for the words "approve" typed into
    /// the chat to resolve anything. The user saw a polite request, granted it,
    /// and watched the identical refusal come back. The turn could only end in
    /// confusion.
    ///
    /// # The card is the agent's own card
    ///
    /// [`crate::pending_user_action`] publishes it into the same session-scoped
    /// queue the agent loop already drains, carrying the same
    /// `ActionRequired::ToolConfirmation` payload — including BR-63's risk grade
    /// and preview — that `handle_approval_tool_requests` yields. So the desktop
    /// draws the dialog it already had, `POST /action-required/tool-confirmation`
    /// resolves it through the fallthrough in `Agent::handle_confirmation`, and
    /// there is one approval UI rather than two that could drift.
    ///
    /// # Every way out is bounded
    ///
    /// * the user decides — allow or deny;
    /// * the turn is cancelled, via **the turn's own token** (Stop,
    ///   `AppState::cancel_turn`, a dropped websocket);
    /// * the lease drops, because the turn ended for any other reason — the
    ///   grant's nonce is the park's owner, so [`BridgeLease::drop`] releases it;
    /// * the TTL elapses, deliberately *shorter* than the child's own per-call
    ///   deadline (see [`approval_ttl`]).
    ///
    /// The last one is the difference between an answer and a hang. Parking past
    /// the child's deadline does not give the user more time; it converts a card
    /// they could still answer into "The operation timed out" — a transport
    /// failure the model may retry, producing a second card for the same call.
    ///
    /// Whatever happens, the child gets a **result**, and the text never invites
    /// an answer that has nowhere to land.
    async fn await_approval(&self, name: &str, pending: &ToolRequest) -> Result<(), String> {
        let Ok(call) = pending.tool_call.as_ref() else {
            return Err(format!("`{name}` needs approval but is not a usable call."));
        };
        let arguments = call.arguments.clone().unwrap_or_default();
        let request = UserActionRequest::ToolApproval(ToolApprovalRequest {
            tool_name: call.name.to_string(),
            arguments: arguments.clone(),
            prompt: Some(format!(
                "{} asked to run this through Biorouter.",
                self.child_label()
            )),
            risk: Some(self.tool_risks.risk_for(&call.name)),
            preview: crate::conversation::tool_preview::ToolPreview::for_tool_call(
                &call.name, &arguments,
            ),
        });

        // Owned by the grant's nonce so the lease can release it, scoped to the
        // session so only that session's loop may surface it (#40).
        //
        // ⚠ An EMPTY session id is `None`, not `Some("")`. A workflow with no
        // chat behind it — a scheduled knowledge macro, a `Session::default()` —
        // carries one, and `Some("")` would key a queue on the empty string that
        // every such run in the process would share, which is #40's
        // cross-session leak with a different key. `None` is the unscoped
        // fallback the manager already has for exactly this case.
        let session_scope = (!self.session.id.is_empty()).then_some(self.session.id.as_str());
        let parked = PendingUserActions::global().park(
            session_scope,
            (!self.nonce.is_empty()).then_some(self.nonce.as_str()),
            request,
        );
        let outcome = parked.wait(approval_ttl(), self.cancel.as_ref()).await;

        match outcome {
            UserActionOutcome::Approved { permission } => {
                tracing::info!(
                    tool_name = %name,
                    ?permission,
                    "a bridged tool call was approved by the user"
                );
                Ok(())
            }
            // `AlwaysDeny` and `DenyOnce` read the same to the child: it may not
            // run this. The *scope* of the refusal is the permission store's
            // business, not the child's.
            UserActionOutcome::Denied { .. } => {
                Err(format!("`{name}` was refused: you did not approve it."))
            }
            other => Err(format!(
                "`{name}` needed a person's approval, and the request {}. \
                 Do not ask again in this reply — a chat message cannot approve it. \
                 Say what you wanted to run and why, and stop.",
                other.refusal_detail()
            )),
        }
    }

    /// What to call the child on the approval card.
    ///
    /// The user is being asked to approve a call *they* did not make, so the card
    /// has to say who did. Read off the session rather than the provider name
    /// because the grant holds no provider handle — and a generic "the agent"
    /// would be indistinguishable from Biorouter's own model asking.
    fn child_label(&self) -> &'static str {
        "The coding agent"
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
    ///
    /// ⚠ The take is scoped to **this call's own request ids**, and that is not a
    /// tidiness point. The staging buffer is keyed by session, and the agent's
    /// session-wide `take_tool_input_rewrites` is safe for the agent only because
    /// `Agent::inspect_and_gate_tool_requests` holds the model's entire batch of
    /// requests when it calls it. The bridge holds exactly one request and is
    /// concurrent — `handle` in `routes/tool_bridge.rs` is a plain axum handler
    /// with no serialization, and both child CLIs issue parallel `tools/call`. So
    /// a session-wide take here is theft: with N bridged calls in flight for one
    /// session, whichever reached this line first would drain all N staged
    /// rewrites, apply the one keyed on its own uuid, and discard the other N-1 —
    /// leaving those calls to dispatch the arguments their hooks had asked to
    /// replace. A hook that sandboxes or redacts a command would then do nothing,
    /// intermittently, with no error and nothing in the transcript to show for
    /// it. Every single-call test in this file passes against that.
    async fn collect_hook_rewrites(
        &self,
        requests: &mut [ToolRequest],
        inspections: &mut Vec<crate::tool_inspection::InspectionResult>,
    ) -> anyhow::Result<()> {
        let ids: Vec<String> = requests.iter().map(|r| r.id.clone()).collect();
        let rewrites = self
            .hooks
            .take_tool_input_rewrites_for(&self.session.id, &ids);
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

    /// BR-19: drop this call's staged hook context, because there is nowhere on
    /// this path to deliver it.
    ///
    /// A PreToolUse or PermissionRequest hook can return `additionalContext` and
    /// `systemMessage` alongside its decision. On the agent's path those are
    /// staged by the inspector and collected at the turn's injection point
    /// (`drain_tool_hook_context`, `agent.rs`), which splices the context into the
    /// model's next message and surfaces the system messages to the user. Neither
    /// destination exists here: the model that made this call lives in another
    /// process with its own context, and the only surface reaching it is the tool
    /// result — which, exactly as with `rewrite_notice` above, must stay data
    /// rather than become a channel for out-of-band prose that every caller
    /// parsing a result would then receive.
    ///
    /// So the honest thing is to drop it, and dropping it deliberately is a
    /// **fix**, not the absence of one. Leaving it staged is worse than either
    /// delivering it or discarding it: the per-session buffer keeps it until the
    /// session next runs an ordinary agent turn, which then drains it and injects
    /// context about a coding-agent tool call that finished minutes ago into an
    /// unrelated turn's transcript — a hook's `systemMessage` surfacing against
    /// the wrong request, attributed to the wrong model. And the buffer is capped
    /// at `MAX_STAGED_TOOL_HOOKS`, so a long bridged turn that never drains starts
    /// evicting its own oldest entries, which is how a rewrite goes missing on a
    /// path that otherwise looks correct.
    ///
    /// Scoped to this call's request id for the same reason the take above is: a
    /// session-wide drain would take entries a concurrent sibling call has staged
    /// and still needs. Logged at debug so an operator chasing "my hook's
    /// additionalContext never appeared under `codex`" finds the answer rather
    /// than silence.
    fn discard_staged_hook_context(&self, request_id: &str) {
        let dropped = self
            .hooks
            .drain_tool_hook_context_for(&self.session.id, &[request_id.to_string()]);
        for staged in dropped {
            if staged.additional_context.is_empty() && staged.system_messages.is_empty() {
                continue;
            }
            tracing::debug!(
                tool = %staged.tool_name,
                context = staged.additional_context.len(),
                messages = staged.system_messages.len(),
                "a hook returned context for a bridged tool call; the child agent's \
                 model is in another process and has no channel to receive it, so it \
                 was dropped rather than left to leak into a later turn"
            );
        }
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
        // #107: revoking the capability is not enough. A call parked on a human
        // is holding the child's HTTP request open, and the turn that would have
        // answered it is over — normally, by panic, or by an early return. Left
        // alone it sits until its TTL, and the card stays on screen offering a
        // decision that can no longer reach anything. Releasing here means the
        // child gets a result at the instant the turn ends.
        let released = PendingUserActions::global().cancel_owner(&self.nonce);
        if released > 0 {
            tracing::debug!(
                released,
                "released bridged calls parked on a human when their turn ended"
            );
        }
    }
}

/// Register a grant and return its lease, or `None` when there is no HTTP server
/// to serve it.
pub fn issue(mut grant: BridgeGrant) -> Option<BridgeLease> {
    let base = base_url()?;
    // 32 hex characters from a v4 uuid: unguessable, and the whole credential.
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let url = format!("{}/tool_bridge/{nonce}", base.trim_end_matches('/'));
    // #107: the grant learns its own nonce here and nowhere else — the nonce
    // cannot exist before the grant it names. It is the *owner* of anything the
    // grant parks on a human, which is what lets the lease below release those
    // parks when the turn ends.
    grant.nonce = nonce.clone();
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
        let _guard = path_jail_lock().await;
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
    ///
    /// ⚠ This one covers the *accessor* and nothing else, and on its own it is
    /// not evidence that the fix works: restoring the original inline
    /// `CancellationToken::new()` at the dispatch site (`call()`, immediately
    /// below `apply_vault`) leaves it — and every other test in this file —
    /// green, because nothing here reaches `dispatch_tool_call`. That is why
    /// [`cancelling_the_turn_reaps_a_tool_a_bridged_call_started`] exists and
    /// why it drives a real process through `call()`. Keep both: this one names
    /// the value, that one proves the wiring, and only the pair distinguishes
    /// "the field round-trips" from "the field is what the tool is dispatched
    /// with".
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

    /// The wiring, end to end: cancelling the turn reaps a process tree a
    /// **bridged** `developer__shell` started.
    ///
    /// The test above asserts that the grant's `Option<CancellationToken>` comes
    /// back out of the accessor. That is not the defect. The defect was an inline
    /// `CancellationToken::new()` at the *dispatch site*, and an accessor test
    /// cannot see it: the accessor can be perfect while `call()` hands
    /// `dispatch_tool_call` a token nobody holds. So this drives a real call
    /// through `call()` — real inspectors, real approval, the real in-process
    /// `developer` extension — and measures the only thing that distinguishes the
    /// two: whether a running process dies.
    ///
    /// The command is issue #72's own repro shape, copied from
    /// `tests/nested_shell_cancellation.rs`: it forks a **grandchild** that sleeps
    /// and then touches `survived`, touches `started` so the test knows the tree
    /// is up, and `wait`s. The grandchild is the point — killing only the direct
    /// child reparents it to init and it runs to completion, so `survived` is a
    /// durable trace of an orphan rather than of a slow shutdown. A dispatch site
    /// that mints its own token leaves the turn's `cancel()` pulling on nothing,
    /// the tree is never reaped, and `survived` appears.
    ///
    /// Unix only, for the `sh -c` command and the process-group kill it is testing.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn cancelling_the_turn_reaps_a_tool_a_bridged_call_started() {
        use std::time::Duration;

        /// How long the orphan sleeps before leaving its trace. Long enough that a
        /// reaped tree cannot possibly reach it, short enough to keep this quick.
        const SURVIVE_AFTER: Duration = Duration::from_secs(4);

        let _guard = path_jail_lock().await;

        let dir = tempfile::tempdir().expect("a temp dir");
        let started = dir.path().join("started");
        let survived = dir.path().join("survived");
        let command = format!(
            "sh -c 'sleep {}; touch \"{}\"' & touch \"{}\"; wait",
            SURVIVE_AFTER.as_secs(),
            survived.display(),
            started.display(),
        );

        let extensions = Arc::new(ExtensionManager::new(
            Arc::new(tokio::sync::Mutex::new(None)),
            Arc::new(crate::session::SessionManager::new(
                dir.path().to_path_buf(),
            )),
        ));
        extensions
            .add_extension(crate::agents::extension::ExtensionConfig::Builtin {
                name: "developer".to_string(),
                description: "developer".to_string(),
                display_name: Some("Developer".to_string()),
                timeout: Some(300),
                bundled: Some(true),
                available_tools: vec![],
            })
            .await
            .expect("the developer extension loads in-process");

        let turn = CancellationToken::new();
        let hooks = no_hooks();
        let grant = Arc::new(BridgeGrant::new(
            Session::default(),
            // Auto, so the permission inspector approves and this test measures
            // cancellation rather than the approval flow.
            BioRouterMode::Auto,
            extensions,
            Arc::new(inspections_with(&hooks, false)),
            test_capability(),
            vec![],
            Conversation::new_unvalidated(vec![]),
            Some(turn.clone()),
            hooks,
            None,
            Arc::new(ToolRiskRegistry::new()),
        ));

        let call = CallToolRequestParams {
            name: "developer__shell".to_string().into(),
            arguments: Some(
                serde_json::json!({ "command": command })
                    .as_object()
                    .expect("an object")
                    .clone(),
            ),
            meta: None,
            task: None,
        };
        let running = tokio::spawn({
            let grant = Arc::clone(&grant);
            async move { grant.call(call).await }
        });

        // Wait for the tree to actually be up; asserting on a command that never
        // started would prove nothing either way.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        while tokio::time::Instant::now() < deadline && !started.exists() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            started.exists(),
            "the bridged shell command never started; the test proves nothing"
        );

        turn.cancel();

        // Give the orphan every chance to surface: wait past its own sleep.
        tokio::time::sleep(SURVIVE_AFTER + Duration::from_secs(3)).await;

        assert!(
            !survived.exists(),
            "cancelling the turn left a descendant of a BRIDGED tool call running: \
             it woke up after the turn ended and wrote {}. The turn's token is not \
             reaching `dispatch_tool_call` — a token constructed at the dispatch \
             site is held by nobody and never fires, so the user pressing stop \
             tears down the child agent and leaves its shell command detached.",
            survived.display()
        );

        // A cancel that hangs is its own bug, and one that never returns would
        // otherwise read here as a pass.
        let ended = tokio::time::timeout(Duration::from_secs(10), running).await;
        assert!(
            ended.is_ok(),
            "the cancelled bridged call never returned to the child"
        );
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
    /// Every starting value here is *hostile* — the flag is left holding the
    /// opposite of the answer before each call — so the test cannot pass by the
    /// flag already happening to hold it.
    ///
    /// All four `BioRouterMode` variants, not the three that are interesting.
    /// `SmartApprove` was missing and its absence was invisible: the policy is
    /// `Auto ⇒ relaxed`, so a mode nobody enumerates is a mode nobody notices
    /// moving to the other side of it, and a `matches!` widened to include it
    /// would take the jail down for a mode whose whole purpose is to ask.
    #[tokio::test]
    async fn a_bridged_call_sets_the_path_jail_from_its_own_mode() {
        // `PATH_JAIL_RELAXED` is process-global, so the grants below have to take
        // turns; a `tokio::test` gives each its own runtime but not its own
        // process.
        let _guard = path_jail_lock().await;

        for (mode, expected) in [
            (BioRouterMode::Auto, true),
            (BioRouterMode::Approve, false),
            (BioRouterMode::SmartApprove, false),
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
        let _guard = path_jail_lock().await;
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
        let _guard = path_jail_lock().await;
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

    /// A bridged call must not take a rewrite that belongs to another call.
    ///
    /// The staging buffer is keyed by *session*, not by call, and the bridge is
    /// the one caller that handles requests one at a time while others for the
    /// same session are in flight (`routes/tool_bridge.rs`'s `handle` is a plain
    /// axum handler with no serialization, and both child CLIs issue parallel
    /// `tools/call`). A session-wide take is therefore theft, and its symptom is
    /// somebody *else's* hook silently doing nothing.
    ///
    /// Written as an assertion about what is LEFT rather than as a race, because
    /// the failure is a race and a race is not a test. The pre-staged entry
    /// stands in for a sibling call that ran its hooks a moment earlier and has
    /// not yet reached `collect_hook_rewrites`; if this call drains it, that
    /// sibling will dispatch its un-rewritten arguments.
    #[tokio::test]
    async fn a_bridged_call_leaves_another_calls_rewrite_staged() {
        let _guard = path_jail_lock().await;
        let fixture = GeneFixture::new().await;

        let hooks = hooks_rewriting_query_to("17");
        // A sibling bridged call, mid-flight: its PreToolUse hook has already run
        // and staged a rewrite against ITS request id, which nothing in this call
        // has ever seen.
        let sibling = "a-concurrent-bridged-call".to_string();
        hooks.stage_tool_hook(
            &Session::default().id,
            crate::hooks::StagedToolHook {
                event: crate::hooks::HookEvent::PreToolUse,
                tool_request_id: sibling.clone(),
                tool_name: "datasql__data_query".to_string(),
                updated_input: Some(query_args("17")),
                additional_context: vec![],
                system_messages: vec![],
            },
        );

        let grant = fixture.grant(inspections_with(&hooks, false), Arc::clone(&hooks));
        let result = grant
            .call(fixture.query_for("7"))
            .await
            .expect("the rewritten query is a valid one");

        // This call still got its own rewrite — the scoping must not have broken
        // the thing it is scoping.
        let text = serde_json::to_string(&result).expect("a serialisable result");
        assert!(
            text.contains("TP53"),
            "this call's own hook rewrite must still apply: {text}"
        );

        let left = hooks.take_tool_input_rewrites(&Session::default().id);
        assert!(
            left.contains_key(&sibling),
            "a bridged call drained a rewrite staged by a different in-flight call \
             and then discarded it, because it is keyed on a request id this call \
             never minted. That sibling will now dispatch the arguments its \
             PreToolUse hook asked to replace — a hook that sandboxes or redacts a \
             command doing nothing, nondeterministically, with no error anywhere. \
             What was left staged: {left:?}"
        );
    }

    /// The same defect as the race it actually is: several bridged calls in one
    /// session at once, every one of which must run its own hook's rewrite.
    ///
    /// This is what happens in production — a child CLI issuing parallel
    /// `tools/call` against one grant — and it is kept alongside the
    /// deterministic test above rather than instead of it, because the two say
    /// different things: that one says the buffer is shared wrong, this one says
    /// the sharing is reachable from the outside.
    ///
    /// ⚠ It **holds the interleaving still**, with a barrier inspector registered
    /// after the hook inspector, and that is not a way of faking a failure. Left
    /// to chance this test passes against the broken code almost every time, for
    /// a reason worth writing down: `HookInspector` is registered last (in
    /// `inspections_with` here, and at `agent.rs`'s "runs last" comment in
    /// production), so a call's staging and its take are separated only by a
    /// return through three frames with no yield point in between. The window is
    /// sub-microsecond, the hook's own `sh -c` is milliseconds, and six tasks
    /// therefore almost never collide. A green run there would be measuring the
    /// registration order, not the correctness of the take — and registration
    /// order is a comment, not an invariant. The barrier releases all six calls
    /// at the one instant the defect needs: every rewrite staged, none yet taken.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_bridged_calls_each_run_their_own_rewrite() {
        const CALLS: usize = 6;

        let _guard = path_jail_lock().await;
        let fixture = GeneFixture::new().await;

        let hooks = hooks_rewriting_query_to("17");
        let mut inspections = inspections_with(&hooks, false);
        inspections.add_inspector(Box::new(BarrierInspector::new(CALLS)));
        let grant = Arc::new(fixture.grant(inspections, Arc::clone(&hooks)));

        let calls: Vec<_> = (0..CALLS)
            .map(|_| {
                let grant = Arc::clone(&grant);
                let call = fixture.query_for("7");
                tokio::spawn(async move { grant.call(call).await })
            })
            .collect();

        for (i, call) in calls.into_iter().enumerate() {
            let result = call
                .await
                .expect("the call task ran")
                .expect("the rewritten query is a valid one");
            let text = serde_json::to_string(&result).expect("a serialisable result");
            assert!(
                text.contains("TP53") && !text.contains("CFTR"),
                "concurrent bridged call {i} ran the model's original chromosome-7 \
                 query: a sibling call drained its staged rewrite before it could \
                 collect it, so the user's PreToolUse hook silently did nothing on \
                 this one call. got: {text}"
            );
        }
    }

    /// A bridged call does not leave its hook context staged for a later turn.
    ///
    /// There is nowhere on this path to deliver a hook's `additionalContext` or
    /// `systemMessage` — the model that made the call is in another process — so
    /// the bridge drops them. Dropping them is the fix; the defect was leaving
    /// them, because the buffer is per-session and the next ordinary agent turn
    /// drains it, injecting context about a coding-agent tool call that finished
    /// minutes ago into an unrelated turn's transcript. (At
    /// `MAX_STAGED_TOOL_HOOKS` entries it also starts evicting its own oldest,
    /// which is how a *rewrite* goes missing on a path that otherwise looks fine.)
    ///
    /// Asserted through `drain_tool_hook_context`, i.e. from exactly where the
    /// later turn would read it.
    #[tokio::test]
    async fn a_bridged_call_leaves_no_hook_context_for_a_later_turn() {
        let _guard = path_jail_lock().await;
        let fixture = GeneFixture::new().await;

        let hooks = hooks_returning(
            "datasql__data_query",
            &serde_json::json!({ "additionalContext": "the cohort is de-identified" }),
        );
        let grant = fixture.grant(inspections_with(&hooks, false), Arc::clone(&hooks));

        grant
            .call(fixture.query_for("7"))
            .await
            .expect("a valid query");

        let leftover = hooks.drain_tool_hook_context(&Session::default().id);
        assert!(
            leftover.is_empty(),
            "a bridged call left {} staged hook effect(s) behind. Nothing on this \
             path delivers them, so they sit in the per-session buffer until the \
             session next runs an ordinary agent turn — which drains them and \
             injects context about somebody else's finished tool call into its own \
             transcript: {leftover:?}",
            leftover.len()
        );
    }

    /// A sibling call's staged context survives too — the drain is scoped for the
    /// same reason the take is.
    #[tokio::test]
    async fn a_bridged_call_leaves_another_calls_hook_context_staged() {
        let _guard = path_jail_lock().await;
        let fixture = GeneFixture::new().await;

        let hooks = no_hooks();
        let sibling = "a-concurrent-bridged-call".to_string();
        hooks.stage_tool_hook(
            &Session::default().id,
            crate::hooks::StagedToolHook {
                event: crate::hooks::HookEvent::PreToolUse,
                tool_request_id: sibling.clone(),
                tool_name: "datasql__data_query".to_string(),
                updated_input: None,
                additional_context: vec!["belongs to another call".to_string()],
                system_messages: vec![],
            },
        );

        let grant = fixture.grant(inspections_with(&hooks, false), Arc::clone(&hooks));
        grant
            .call(fixture.query_for("7"))
            .await
            .expect("a valid query");

        let left = hooks.drain_tool_hook_context(&Session::default().id);
        assert!(
            left.iter().any(|s| s.tool_request_id == sibling),
            "a bridged call dropped a staged effect belonging to a different \
             in-flight call: {left:?}"
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
        let _guard = path_jail_lock().await;
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
        let _guard = path_jail_lock().await;
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
                Arc::clone(&self.extensions) as Arc<dyn BridgeToolDispatch>,
                Arc::new(inspections),
                test_capability(),
                vec![],
                Conversation::new_unvalidated(vec![]),
                None,
                hooks,
                vault,
                Arc::new(ToolRiskRegistry::new()),
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
        hooks_returning(
            "datasql__data_query",
            &serde_json::json!({ "updatedInput": query_args(chrom) }),
        )
    }

    fn hooks_rewriting_shell_to(command: &str) -> Arc<crate::hooks::HooksManager> {
        hooks_returning(
            "developer__shell",
            &serde_json::json!({ "updatedInput": { "command": command } }),
        )
    }

    /// An inspector that decides nothing and parks every call until `n` of them
    /// have arrived.
    ///
    /// Registered *after* the hook inspector, so each call has already staged its
    /// PreToolUse rewrite when it parks. Releasing all of them together produces,
    /// on purpose and every run, the one interleaving a session-wide
    /// `take_tool_input_rewrites` mishandles: N rewrites staged for one session,
    /// N callers about to take, each entitled to exactly one of them.
    ///
    /// It exists because the alternative is a test whose verdict depends on the
    /// scheduler. The interleaving it forces is reachable in production without
    /// any help — the bridge's HTTP handler has no serialization and both child
    /// CLIs issue parallel `tools/call` — it is simply rare enough at this
    /// inspector ordering that chance would report "fixed" against broken code.
    ///
    /// It parks the **first** pass only. `collect_hook_rewrites` re-inspects a
    /// call whose arguments a hook rewrote, and that second pass excludes only
    /// the hook inspector — so a barrier that parked every pass would be waiting
    /// for a quorum that depends on how many calls got a rewrite, which is
    /// precisely the quantity under test. Against the broken code exactly one
    /// call re-inspects, and the test would hang instead of failing.
    struct BarrierInspector {
        barrier: tokio::sync::Barrier,
        released: std::sync::atomic::AtomicBool,
    }

    impl BarrierInspector {
        fn new(parties: usize) -> Self {
            Self {
                barrier: tokio::sync::Barrier::new(parties),
                released: std::sync::atomic::AtomicBool::new(false),
            }
        }
    }

    #[async_trait::async_trait]
    impl crate::tool_inspection::ToolInspector for BarrierInspector {
        fn name(&self) -> &'static str {
            "test-barrier"
        }

        async fn inspect(
            &self,
            _tool_requests: &[ToolRequest],
            _messages: &[crate::conversation::message::Message],
            _biorouter_mode: BioRouterMode,
            _session: &Session,
        ) -> anyhow::Result<Vec<crate::tool_inspection::InspectionResult>> {
            use std::sync::atomic::Ordering;
            if !self.released.load(Ordering::SeqCst) {
                self.barrier.wait().await;
                self.released.store(true, Ordering::SeqCst);
            }
            Ok(vec![])
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    /// A `HooksManager` holding one PreToolUse hook that prints the given
    /// `hookSpecificOutput` for `matcher` — a rewrite, some `additionalContext`,
    /// or anything else that block carries.
    ///
    /// A real shell command rather than a stub, because the staging happens
    /// inside the manager as a side effect of the hook *running*; a fake that
    /// staged directly would test the assertion rather than the path.
    fn hooks_returning(
        matcher: &str,
        hook_specific_output: &serde_json::Value,
    ) -> Arc<crate::hooks::HooksManager> {
        let payload = serde_json::json!({ "hookSpecificOutput": hook_specific_output }).to_string();
        let command = if cfg!(target_os = "windows") {
            // Command hooks run through cmd.exe on Windows. Unlike sh, cmd keeps
            // the JSON's double quotes intact and would echo literal single
            // quotes, making the hook output invalid JSON.
            format!("echo {payload}")
        } else {
            // Single-quoted for `sh -c`, with any embedded quote escaped the
            // usual way. The JSON above contains none, but a future edit to the
            // fixtures should not become a mysterious hook failure.
            let quoted = payload.replace('\'', "'\"'\"'");
            format!("echo '{quoted}'")
        };
        let yaml = format!(
            "PreToolUse:\n  - matcher: {}\n    hooks:\n      - type: command\n        command: {}\n",
            serde_json::to_string(matcher).expect("a json string"),
            serde_json::to_string(&command).expect("a json string"),
        );
        let config = serde_yaml::from_str(&yaml).expect("the hook config parses");
        Arc::new(crate::hooks::HooksManager::with_config(
            config,
            false,
            Arc::new(tokio::sync::Mutex::new(None)),
        ))
    }

    /// Serializes every test in this module that touches the process-global path
    /// jail.
    ///
    /// `tokio::sync::Mutex` rather than `std::sync::Mutex`, and not as a style
    /// preference: every holder of this guard awaits while holding it (that is
    /// the point — the whole `call()` has to run without another test moving the
    /// flag underneath it), and a `std` guard held across an await blocks the
    /// runtime's worker thread. It also has no poisoning to recover from, which
    /// is right here: the guarded value is `()`, so a panicking test leaves
    /// nothing half-updated and refusing to run the next test would turn one
    /// failure into two.
    async fn path_jail_lock() -> tokio::sync::MutexGuard<'static, ()> {
        static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        LOCK.lock().await
    }

    /// The privacy capability every grant in this module is built with — the
    /// most restrictive pair the type can express, never a permissive one
    /// invented for a test's convenience.
    ///
    /// ⚠ **Call this rather than `CallCapability::public_enforced()` directly**,
    /// and the reason is a gate rather than a style preference.
    /// `tests/privacy_capability.rs` runs a repo-wide grep census over every site
    /// that decides how far a caller reaches, deliberately line-wise so that it
    /// cannot tell a `#[cfg(test)]` block from production — a filter that could
    /// would blind it to production too. It therefore counts this file's test
    /// helpers, and its documented row for this file says "one, and it is a test
    /// helper". Four inline spellings had accumulated here against that row,
    /// which is why `cargo test -p biorouter --test privacy_capability` was RED
    /// while every other gate on this branch was green.
    ///
    /// Funnelling them through one named function keeps the census's number
    /// stable as tests are added, without weakening it in the slightest: a
    /// genuinely new decider in this file — or a new test that inlines the
    /// constructor instead of calling this — still moves the count off 1 and
    /// still fires.
    fn test_capability() -> CallCapability {
        CallCapability::public_enforced()
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
            test_capability(),
            vec![],
            Conversation::new_unvalidated(vec![]),
            None,
            no_hooks(),
            None,
            Arc::new(ToolRiskRegistry::new()),
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

    // ---------------------------------------------------------------------
    // #107: a call that needs a person
    // ---------------------------------------------------------------------

    /// A session with its own id.
    ///
    /// `Session::default()` has an EMPTY id, and the action-required queue is
    /// keyed by session id in a process-global map — so every default-session
    /// test would share one queue and drain each other's cards. The bug that
    /// would hide is precisely the one #40 fixed for the agent's own path.
    fn session_named(id: &str) -> Session {
        Session {
            id: id.to_string(),
            ..Session::default()
        }
    }

    /// A grant over the datasql fixture, in a mode whose permission inspector
    /// routes an unapproved call to `needs_approval`.
    fn approving_grant(db: &GeneFixture, session: Session) -> Arc<BridgeGrant> {
        let hooks = no_hooks();
        Arc::new(BridgeGrant::new(
            session,
            BioRouterMode::Approve,
            Arc::clone(&db.extensions) as Arc<dyn BridgeToolDispatch>,
            Arc::new(inspections_with(&hooks, false)),
            test_capability(),
            vec![],
            Conversation::new_unvalidated(vec![]),
            None,
            hooks,
            None,
            Arc::new(ToolRiskRegistry::new()),
        ))
    }

    /// The id on the approval card queued for `session_id`, once it appears.
    ///
    /// Polls rather than waits on a notify: the card is published from inside a
    /// spawned `grant.call(...)`, so a single unconditional drain races it.
    async fn approval_card_id(session_id: &str) -> String {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let drained =
                    crate::action_required_manager::ActionRequiredManager::global()
                        .drain_requests(session_id);
                for message in &drained {
                    for content in &message.content {
                        if let crate::conversation::message::MessageContent::ActionRequired(a) =
                            content
                        {
                            if let crate::conversation::message::ActionRequiredData::ToolConfirmation {
                                id,
                                ..
                            } = &a.data
                            {
                                return id.clone();
                            }
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("an approval card must be published for the session")
    }

    /// The whole of #107: a bridged call that needs approval raises a real card,
    /// parks, and RUNS once the user allows it.
    ///
    /// Before this, the same call came back with "needs a person's approval, and
    /// this turn has no way to ask for one" — advice to ask in prose, which the
    /// model followed, for an approval no client could ever deliver.
    #[tokio::test]
    async fn an_approval_parks_the_call_and_runs_it_once_allowed() {
        let db = GeneFixture::new().await;
        let session = session_named(&format!("approve-ok-{}", uuid::Uuid::new_v4()));
        let session_id = session.id.clone();
        let grant = approving_grant(&db, session);

        let call = db.query_for("1");
        let running = tokio::spawn({
            let grant = Arc::clone(&grant);
            async move { grant.call(call).await }
        });

        let id = approval_card_id(&session_id).await;
        assert_eq!(
            PendingUserActions::global().resolve(
                &id,
                UserActionOutcome::Approved {
                    permission: crate::permission::Permission::AllowOnce,
                },
            ),
            crate::pending_user_action::ResolveOutcome::Delivered,
        );

        let result = tokio::time::timeout(Duration::from_secs(10), running)
            .await
            .expect("the approved call must resume promptly, not at its TTL")
            .expect("the call task must not panic")
            .expect("an approved call runs");
        assert_ne!(result.is_error, Some(true), "the tool ran: {result:?}");
    }

    /// The card the bridge raises is the card the desktop already draws — same
    /// `ActionRequired::ToolConfirmation` payload, carrying BR-63's risk grade,
    /// so there is one approval dialog rather than two that can drift.
    #[tokio::test]
    async fn the_card_is_the_agents_own_confirmation_shape() {
        let db = GeneFixture::new().await;
        let session = session_named(&format!("approve-card-{}", uuid::Uuid::new_v4()));
        let session_id = session.id.clone();
        let grant = approving_grant(&db, session);

        let call = db.query_for("2");
        let running = tokio::spawn({
            let grant = Arc::clone(&grant);
            async move { grant.call(call).await }
        });

        let card = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let drained = crate::action_required_manager::ActionRequiredManager::global()
                    .drain_requests(&session_id);
                if let Some(message) = drained.into_iter().next() {
                    return message;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("a card");

        let mut saw = false;
        for content in &card.content {
            if let crate::conversation::message::MessageContent::ActionRequired(a) = content {
                if let crate::conversation::message::ActionRequiredData::ToolConfirmation {
                    id,
                    tool_name,
                    arguments,
                    risk,
                    ..
                } = &a.data
                {
                    saw = true;
                    assert!(!id.is_empty(), "the card must carry a routable request id");
                    assert_eq!(tool_name, "datasql__data_query");
                    assert!(
                        arguments.contains_key("sql"),
                        "the user has to see what they are approving"
                    );
                    assert!(risk.is_some(), "BR-63's grade must survive the bridge");
                }
            }
        }
        assert!(
            saw,
            "the published message must be a tool-confirmation card"
        );
        running.abort();
    }

    /// A denial comes back as an ordinary tool result the model can act on, and
    /// its text must not send the model back to ask in prose — the request id is
    /// gone, so a chat message could not resolve anything.
    #[tokio::test]
    async fn a_denial_is_a_result_that_does_not_invite_a_chat_answer() {
        let db = GeneFixture::new().await;
        let session = session_named(&format!("approve-deny-{}", uuid::Uuid::new_v4()));
        let session_id = session.id.clone();
        let grant = approving_grant(&db, session);

        let call = db.query_for("3");
        let running = tokio::spawn({
            let grant = Arc::clone(&grant);
            async move { grant.call(call).await }
        });

        let id = approval_card_id(&session_id).await;
        PendingUserActions::global().resolve(
            &id,
            UserActionOutcome::Denied {
                permission: crate::permission::Permission::DenyOnce,
            },
        );

        let refusal = tokio::time::timeout(Duration::from_secs(10), running)
            .await
            .expect("a denial must resume the call promptly")
            .expect("no panic")
            .expect_err("a denied call must not run");
        assert!(
            refusal.contains("did not approve"),
            "the model must learn it was refused: {refusal}"
        );
        assert!(
            !refusal.contains("let them approve"),
            "the old text invited an answer that cannot land: {refusal}"
        );
    }

    /// The turn's own cancel token releases a parked call — Stop, `cancel_turn`
    /// and a dropped websocket all reach it through that one token.
    #[tokio::test]
    async fn cancelling_the_turn_releases_a_call_parked_on_a_person() {
        let db = GeneFixture::new().await;
        let session = session_named(&format!("approve-cancel-{}", uuid::Uuid::new_v4()));
        let session_id = session.id.clone();
        let token = CancellationToken::new();
        let hooks = no_hooks();
        let grant = Arc::new(BridgeGrant::new(
            session,
            BioRouterMode::Approve,
            Arc::clone(&db.extensions) as Arc<dyn BridgeToolDispatch>,
            Arc::new(inspections_with(&hooks, false)),
            test_capability(),
            vec![],
            Conversation::new_unvalidated(vec![]),
            Some(token.clone()),
            hooks,
            None,
            Arc::new(ToolRiskRegistry::new()),
        ));

        let call = db.query_for("4");
        let running = tokio::spawn({
            let grant = Arc::clone(&grant);
            async move { grant.call(call).await }
        });
        // Wait until the card is up, so the cancel lands on a genuinely parked
        // call rather than before it ever parked.
        let _ = approval_card_id(&session_id).await;
        token.cancel();

        let refusal = tokio::time::timeout(Duration::from_secs(10), running)
            .await
            .expect("a cancelled turn must release the park immediately, not at its TTL")
            .expect("no panic")
            .expect_err("a cancelled approval must not run the tool");
        assert!(
            refusal.contains("cancelled"),
            "the child must be told why: {refusal}"
        );
    }

    /// Dropping the lease is the other way a turn ends — a panic, an early
    /// return, a provider error. It must release the park too, or the child sits
    /// on an HTTP response nobody will ever answer.
    #[tokio::test]
    async fn dropping_the_lease_releases_a_call_parked_on_a_person() {
        publish_base_url("http://127.0.0.1:65535");
        let db = GeneFixture::new().await;
        let session = session_named(&format!("approve-lease-{}", uuid::Uuid::new_v4()));
        let session_id = session.id.clone();
        let hooks = no_hooks();
        let lease = issue(BridgeGrant::new(
            session,
            BioRouterMode::Approve,
            Arc::clone(&db.extensions) as Arc<dyn BridgeToolDispatch>,
            Arc::new(inspections_with(&hooks, false)),
            test_capability(),
            vec![],
            Conversation::new_unvalidated(vec![]),
            None,
            hooks,
            None,
            Arc::new(ToolRiskRegistry::new()),
        ))
        .expect("a base url is published");
        let nonce = lease
            .url()
            .rsplit('/')
            .next()
            .expect("the url ends in the nonce")
            .to_string();
        let grant = lookup(&nonce).expect("the grant is live");

        let call = db.query_for("5");
        let running = tokio::spawn(async move { grant.call(call).await });
        let _ = approval_card_id(&session_id).await;

        drop(lease);

        let refusal = tokio::time::timeout(Duration::from_secs(10), running)
            .await
            .expect("the lease drop must release the park, not leave it to its TTL")
            .expect("no panic")
            .expect_err("the tool must not run");
        assert!(
            refusal.contains("cancelled"),
            "the child must be told why: {refusal}"
        );
    }

    /// The approval window has to fit INSIDE the child's own per-call deadline.
    /// Parking past it does not give the user more time — it turns a card they
    /// could still answer into "The operation timed out", a transport failure the
    /// model retries, producing a second card for the same call (#110).
    #[test]
    fn the_approval_window_fits_inside_the_childs_deadline() {
        assert!(
            approval_ttl() < child_tool_call_budget(),
            "an approval that outlives the transport is a hang, not a prompt"
        );
        assert!(
            approval_ttl() >= Duration::from_secs(30),
            "a window this short is not a decision, it is a race"
        );
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
            test_capability(),
            vec![],
            Conversation::new_unvalidated(vec![]),
            cancel,
            no_hooks(),
            None,
            Arc::new(ToolRiskRegistry::new()),
        )
    }
}
