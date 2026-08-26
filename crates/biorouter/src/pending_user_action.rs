//! One parked call, one human decision — the rendezvous a tool call uses when
//! it cannot proceed without a person (#107).
//!
//! # Why this exists
//!
//! Biorouter already had two park/resume mechanics and they did not compose:
//!
//! * **Tool permission** — [`crate::agents::Agent::register_confirmation`] hands
//!   the reply loop a `oneshot`, the loop yields a confirmation card, and
//!   `POST /action-required/tool-confirmation` resolves it. It lives on the
//!   `Agent`, so only code holding an `&Agent` can raise one.
//! * **MCP elicitation** — [`crate::action_required_manager`] queues a request
//!   message per session and parks the caller. It is process-global, so anyone
//!   can raise one, but it can only ask for *data*, never for a permission
//!   decision, and it has no way to collect a value without that value passing
//!   through the conversation transport.
//!
//! A bridged coding-agent tool call needs both halves and can reach neither: it
//! runs on an axum task inside `POST /tool_bridge/{nonce}`, with a
//! [`crate::providers::coding_agent::bridge::BridgeGrant`] snapshot and no
//! `Agent` handle at all. Until this module existed the bridge answered
//! `needs_approval` with a refusal, and the model dutifully asked the user in
//! prose — a question nothing could answer, because no request id had ever been
//! minted.
//!
//! So this is the one registry both mechanics now route through, and the one
//! type a new surface should reach for rather than inventing a third.
//!
//! # The shape
//!
//! ```text
//! park(session, owner, request)  ->  PendingUserAction   (publishes the card)
//!        .wait(ttl, cancel)      ->  UserActionOutcome    (approve/deny/data/
//!                                                          secrets/cancel/timeout)
//! resolve_in_session(session, id, outcome) <- the session surface that showed it
//! ```
//!
//! Publication deliberately reuses [`crate::action_required_manager`]'s
//! session-scoped queue rather than adding a second one. That queue already has
//! the only wake seam an agent loop watches (`request_arrived`) and the only
//! drain that persists a request under the right session id, and a second
//! channel would mean a loop had to race two notifies — the exact shape that
//! made #40 a cross-session prompt leak.
//!
//! # Resolution is keyed per request, and that is a safety property
//!
//! Every park mints its own uuid and its own `oneshot`. A decision for an id
//! nobody is waiting on is dropped and reported as [`ResolveOutcome::Unknown`],
//! never applied to whichever call happens to be parked now. Two bridged calls
//! from the same child, in flight at the same time (both CLIs issue parallel
//! `tools/call`), therefore cannot resolve each other — the property BR-62
//! established for the agent's own path, extended to this one.
//!
//! # Secret-safe resolution
//!
//! [`UserActionRequest::Secrets`] asks a *trusted surface* to collect values and
//! write them straight to the keyring. The parked caller learns only which keys
//! were configured, because that is the only thing
//! [`UserActionOutcome::SecretsConfigured`] can carry — there is no field for a
//! value, the published card carries only key names and labels, and
//! [`PendingUserActions::resolve`] **refuses** a data-bearing outcome for a
//! secrets request rather than letting a mis-wired route smuggle one into the
//! transcript. A secret that never enters the conversation transport cannot be
//! persisted into a session row, replayed into a later prompt, or flattened into
//! a child agent's transcript.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use uuid::Uuid;

use rmcp::model::JsonObject;

use crate::conversation::message::{Message, MessageContent, SecretDestination, SecretKeyRequest};
use crate::conversation::tool_preview::ToolPreview;
use crate::permission::tool_risk::ToolRisk;
use crate::permission::Permission;

/// A tool call the permission inspector routed to `needs_approval`.
#[derive(Debug, Clone)]
pub struct ToolApprovalRequest {
    /// The tool as the user will see it named on the card. The bridge strips no
    /// prefix here: a bridged call's name is the one the child asked for, and a
    /// card naming something else would be a card about a different call.
    pub tool_name: String,
    pub arguments: JsonObject,
    /// Why approval is being asked for, when the inspector said.
    pub prompt: Option<String>,
    /// BR-63's risk grade, so the card can say *how* dangerous the call is.
    pub risk: Option<ToolRisk>,
    /// BR-63's preview — the resolved command, the diff — so the decision is
    /// informed rather than a name and a shrug.
    pub preview: Option<ToolPreview>,
}

/// An ordinary MCP elicitation: free-form data described by a JSON schema.
#[derive(Debug, Clone)]
pub struct ElicitationRequest {
    pub message: String,
    pub requested_schema: Value,
}

/// Values that must never enter the conversation transport.
///
/// The surface that answers this writes the values straight to their
/// destination (the OS credential store, an extension's env) and resolves with
/// [`UserActionOutcome::SecretsConfigured`], which carries key *names* only.
#[derive(Debug, Clone)]
pub struct SecretsRequest {
    /// What the values are for, in the user's terms.
    pub prompt: String,
    /// The keys to collect. Order is the order the card should show them in.
    pub keys: Vec<SecretKeyRequest>,
    /// Where the trusted surface must put them.
    pub destination: SecretDestination,
}

/// What a parked call is asking a human for.
#[derive(Debug, Clone)]
pub enum UserActionRequest {
    ToolApproval(ToolApprovalRequest),
    Elicitation(ElicitationRequest),
    Secrets(SecretsRequest),
}

impl UserActionRequest {
    /// A short label for logs and refusal text. Never includes arguments, a
    /// schema, or anything from a secrets request beyond its key names.
    pub fn describe(&self) -> String {
        match self {
            Self::ToolApproval(r) => format!("approval for `{}`", r.tool_name),
            Self::Elicitation(_) => "input".to_string(),
            Self::Secrets(r) => format!("{} credential(s)", r.keys.len()),
        }
    }

    /// Whether an outcome is one this request could legitimately produce.
    ///
    /// The load-bearing case is [`UserActionRequest::Secrets`] refusing
    /// [`UserActionOutcome::Provided`]: `Provided` carries a `Value`, and a
    /// route that answered a secrets card with one would put the credential on
    /// the same path every other tool result takes — persisted to the session
    /// row, replayed into the next prompt, flattened into a child agent's
    /// transcript. Refusing it here means the guarantee holds for surfaces this
    /// module has never heard of.
    fn accepts(&self, outcome: &UserActionOutcome) -> bool {
        matches!(
            (self, outcome),
            // Every request can end without an answer.
            (
                _,
                UserActionOutcome::Cancelled
                    | UserActionOutcome::TimedOut
                    | UserActionOutcome::Failed { .. }
            ) | (
                Self::ToolApproval(_),
                UserActionOutcome::Approved { .. } | UserActionOutcome::Denied { .. }
            ) | (Self::Elicitation(_), UserActionOutcome::Provided { .. })
                | (
                    Self::Secrets(_),
                    UserActionOutcome::SecretsConfigured { .. }
                )
        )
    }
}

/// How a parked call was released.
///
/// Every variant is a *decision*, including the three that are not the user's:
/// a caller matches on this and always has something to tell the model, which is
/// the difference between this and the timeout error it replaced.
#[derive(Debug, Clone, PartialEq)]
pub enum UserActionOutcome {
    /// The user allowed the call. `permission` distinguishes a one-off from an
    /// `AlwaysAllow` the caller may want to record.
    Approved { permission: Permission },
    /// The user refused. Also carries the flavour (`DenyOnce` / `AlwaysDeny`).
    Denied { permission: Permission },
    /// An elicitation was answered.
    Provided { data: Value },
    /// A secrets request was satisfied. **Names only, by construction** — see
    /// the module header.
    SecretsConfigured { configured_keys: Vec<String> },
    /// The user dismissed it, the turn was cancelled, or an unattended run
    /// could never have collected an answer.
    Cancelled,
    /// The time-to-live elapsed with nobody answering.
    TimedOut,
    /// The request could not be put to a human at all.
    Failed { reason: String },
}

impl UserActionOutcome {
    /// Whether the parked call may now proceed.
    pub fn is_allowed(&self) -> bool {
        matches!(
            self,
            Self::Approved { .. } | Self::Provided { .. } | Self::SecretsConfigured { .. }
        )
    }

    /// A sentence for the caller to hand back to whatever asked, safe to show a
    /// model. Deliberately never suggests that a plain chat message can approve
    /// anything: on the bridged path the request id is gone by the time this is
    /// read, so an instruction to "ask the user to approve" would be asking for
    /// an answer with nowhere to land (#107).
    pub fn refusal_detail(&self) -> &'static str {
        match self {
            Self::Approved { .. } | Self::Provided { .. } | Self::SecretsConfigured { .. } => {
                "was allowed"
            }
            Self::Denied { .. } => "was refused by the user",
            Self::Cancelled => "was cancelled before anyone answered it",
            Self::TimedOut => "expired before anyone answered it",
            Self::Failed { .. } => "could not be put to a person",
        }
    }
}

/// What [`PendingUserActions::resolve`] did with a decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveOutcome {
    /// A live parked caller took it.
    Delivered,
    /// Nothing is waiting on that id: a double-click, a card answered after the
    /// turn ended, a stale client. Dropped, never re-aimed at another call.
    Unknown,
    /// The id exists but this outcome is not one that request can produce —
    /// today, a data-bearing answer to a secrets card. The caller stays parked;
    /// the surface has a bug.
    Rejected,
}

struct Entry {
    session_id: Option<String>,
    owner: Option<String>,
    request: UserActionRequest,
    tx: Option<oneshot::Sender<UserActionOutcome>>,
}

/// The process-global registry of parked user actions.
///
/// Process-global for the same reason [`crate::action_required_manager`] is:
/// the surface that answers runs on a different task from the caller that
/// parked, often in a different subsystem, and on the bridged path there is no
/// `Agent` for either of them to share.
#[derive(Default)]
pub struct PendingUserActions {
    entries: Mutex<HashMap<String, Entry>>,
}

impl PendingUserActions {
    /// The registry every production caller uses.
    ///
    /// An `Arc` rather than a bare `&'static` because [`PendingUserAction`]
    /// holds its registry: `park`, `wait` and `Drop` must all act on the *same*
    /// one, and a handle that assumed the global would silently split a test's
    /// standalone registry in half — parking in one and deregistering from the
    /// other.
    pub fn global() -> &'static Arc<Self> {
        static INSTANCE: once_cell::sync::Lazy<Arc<PendingUserActions>> =
            once_cell::sync::Lazy::new(|| Arc::new(PendingUserActions::default()));
        &INSTANCE
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Register `request`, publish its card to `session_id`'s queue, and return
    /// the handle to park on.
    ///
    /// Registration happens **before** publication, deliberately: a surface fast
    /// enough to answer the card before this function returns must still find a
    /// live sender. That is BR-62's ordering, and the reason it exists is that
    /// the opposite order loses exactly the decisions made by a user who was
    /// already looking at the screen.
    ///
    /// `session_id` is the delivery scope — the session whose loop may surface
    /// this. `None` queues it unscoped, deliverable by any loop, for callers
    /// that genuinely cannot attribute it.
    ///
    /// `owner` groups actions that must die together: a bridge lease passes its
    /// nonce, so [`Self::cancel_owner`] releases every call parked under a turn
    /// that has ended. `None` means only the session scope and the TTL bound it.
    pub fn park(
        self: &Arc<Self>,
        session_id: Option<&str>,
        owner: Option<&str>,
        request: UserActionRequest,
    ) -> PendingUserAction {
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.lock().insert(
            id.clone(),
            Entry {
                session_id: session_id.map(str::to_string),
                owner: owner.map(str::to_string),
                request: request.clone(),
                tx: Some(tx),
            },
        );

        crate::action_required_manager::ActionRequiredManager::global()
            .publish(session_id, request_message(&id, &request));

        PendingUserAction {
            id,
            request,
            rx: Some(rx),
            registry: Arc::clone(self),
        }
    }

    /// Deliver a decision from the exact session that owns the parked call.
    ///
    /// The scope comparison and sender take happen in one critical section. A
    /// caller that knows an id but posts it from another session therefore sees
    /// the same [`ResolveOutcome::Unknown`] as a stale id, and the real waiter
    /// remains parked for its own surface.
    pub fn resolve_in_session(
        &self,
        session_id: &str,
        id: &str,
        outcome: UserActionOutcome,
    ) -> ResolveOutcome {
        self.resolve_matching(id, outcome, |entry| {
            entry.session_id.as_deref() == Some(session_id)
        })
    }

    /// Resolve a credential card from the trusted, proof-of-user surface that
    /// predates session-bearing credential submissions.
    ///
    /// This intentionally cannot resolve approvals or elicitations. Keep it
    /// crate-private: an HTTP/model-facing caller must use
    /// [`Self::resolve_in_session`] instead.
    pub(crate) fn resolve_trusted_sessionless_secret(
        &self,
        id: &str,
        outcome: UserActionOutcome,
    ) -> ResolveOutcome {
        self.resolve_matching(id, outcome, |entry| {
            matches!(&entry.request, UserActionRequest::Secrets(_))
        })
    }

    fn resolve_matching(
        &self,
        id: &str,
        outcome: UserActionOutcome,
        scope_matches: impl FnOnce(&Entry) -> bool,
    ) -> ResolveOutcome {
        let mut entries = self.lock();
        let Some(entry) = entries.get_mut(id) else {
            debug!("No parked user action is waiting on {id}; dropping the decision");
            return ResolveOutcome::Unknown;
        };
        if !scope_matches(entry) {
            debug!("No parked user action is waiting on {id}; dropping the decision");
            return ResolveOutcome::Unknown;
        }
        if !entry.request.accepts(&outcome) {
            debug!(
                "Refusing a {outcome:?} for {id}, which asked for {}",
                entry.request.describe()
            );
            return ResolveOutcome::Rejected;
        }
        let Some(tx) = entry.tx.take() else {
            return ResolveOutcome::Unknown;
        };
        entries.remove(id);
        drop(entries);
        match tx.send(outcome) {
            Ok(()) => ResolveOutcome::Delivered,
            // The waiter went away between the lookup and the send — the turn
            // ended. Nothing to do and nothing to blame.
            Err(_) => ResolveOutcome::Unknown,
        }
    }

    /// Bind an unscoped card to the first session that drains it.
    ///
    /// The unscoped queue itself is single-consumer, and this claim happens
    /// before the drained message is returned to that consumer. Once claimed,
    /// every response must pass [`Self::resolve_in_session`].
    pub(crate) fn claim_unscoped_for_session(&self, id: &str, session_id: &str) {
        let mut entries = self.lock();
        let Some(entry) = entries.get_mut(id) else {
            return;
        };
        if entry.session_id.is_none() {
            entry.session_id = Some(session_id.to_string());
        }
    }

    /// Whether anything is parked on `id`. Lets a route answer a duplicate POST
    /// honestly instead of pretending it resolved something.
    pub fn is_pending(&self, id: &str) -> bool {
        self.lock().contains_key(id)
    }

    /// The request parked under `id`, if any. A surface uses this to decide
    /// *which* dialog to draw before it answers.
    pub fn peek(&self, id: &str) -> Option<UserActionRequest> {
        self.lock().get(id).map(|e| e.request.clone())
    }

    /// Cancel every action parked under `owner`, returning how many were
    /// released. Called when a bridge lease drops, so a turn that ended — normally,
    /// by panic, or by an early return — cannot leave a child blocked on an HTTP
    /// response nobody will ever answer.
    pub fn cancel_owner(&self, owner: &str) -> usize {
        self.cancel_matching(|entry| entry.owner.as_deref() == Some(owner))
    }

    /// Cancel every action parked for `session_id`. Called on session stop.
    pub fn cancel_session(&self, session_id: &str) -> usize {
        self.cancel_matching(|entry| entry.session_id.as_deref() == Some(session_id))
    }

    fn cancel_matching(&self, predicate: impl Fn(&Entry) -> bool) -> usize {
        let senders: Vec<oneshot::Sender<UserActionOutcome>> = {
            let mut entries = self.lock();
            let ids: Vec<String> = entries
                .iter()
                .filter(|(_, entry)| predicate(entry))
                .map(|(id, _)| id.clone())
                .collect();
            ids.into_iter()
                .filter_map(|id| entries.remove(&id).and_then(|mut e| e.tx.take()))
                .collect()
        };
        let mut released = 0;
        for tx in senders {
            if tx.send(UserActionOutcome::Cancelled).is_ok() {
                released += 1;
            }
        }
        released
    }

    /// Drop the entry for `id` without a decision. Idempotent.
    fn forget(&self, id: &str) {
        self.lock().remove(id);
    }
}

/// A published request, and the caller's end of its rendezvous.
///
/// Dropping this without [`Self::wait`] deregisters the request, so a caller
/// that bails cannot leave a decision routable to a receiver nobody holds.
pub struct PendingUserAction {
    id: String,
    request: UserActionRequest,
    rx: Option<oneshot::Receiver<UserActionOutcome>>,
    registry: Arc<PendingUserActions>,
}

impl PendingUserAction {
    /// The id the answering surface posts back. Also the id on the published
    /// card.
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn request(&self) -> &UserActionRequest {
        &self.request
    }

    /// Park until a human answers, `ttl` elapses, or `cancel` trips.
    ///
    /// `cancel` is the **turn's** token, not one made here: every cancellation
    /// mechanism Biorouter has — Stop, `AppState::cancel_turn`, the websocket
    /// `TurnGuard` — reaches a running call through that one token, so a token
    /// minted at this call site would leave the user's Stop pulling on nothing
    /// while the child sat on an HTTP response for the full TTL.
    ///
    /// Always returns an outcome; there is no error path. A caller that cannot
    /// tell "denied" from "the machinery broke" would have to invent a policy
    /// for the difference, and the fail-safe policy is the same either way.
    pub async fn wait(
        mut self,
        ttl: Duration,
        cancel: Option<&CancellationToken>,
    ) -> UserActionOutcome {
        let Some(rx) = self.rx.take() else {
            return UserActionOutcome::Failed {
                reason: "this request was already awaited".to_string(),
            };
        };

        let outcome = tokio::select! {
            biased;
            () = async {
                match cancel {
                    Some(token) => token.cancelled().await,
                    None => std::future::pending::<()>().await,
                }
            } => UserActionOutcome::Cancelled,
            answered = tokio::time::timeout(ttl, rx) => match answered {
                Ok(Ok(outcome)) => outcome,
                // The registry entry was dropped without a decision.
                Ok(Err(_)) => UserActionOutcome::Cancelled,
                Err(_) => UserActionOutcome::TimedOut,
            },
        };

        // On every path but a delivered decision the entry is still registered;
        // clearing it here is what stops a late click resolving a receiver that
        // has already gone.
        self.registry.forget(&self.id);
        outcome
    }
}

impl Drop for PendingUserAction {
    fn drop(&mut self) {
        if self.rx.is_some() {
            self.registry.forget(&self.id);
        }
    }
}

/// The card a surface renders for `request`.
///
/// Each variant maps onto the `ActionRequired` shape the desktop already
/// understands, so an approval raised from the bridge draws the *same* dialog as
/// one raised by the agent's own loop rather than a second, subtly different
/// one.
fn request_message(id: &str, request: &UserActionRequest) -> Message {
    let content = match request {
        UserActionRequest::ToolApproval(r) => MessageContent::action_required_with_context(
            id,
            r.tool_name.clone(),
            r.arguments.clone(),
            r.prompt.clone(),
            r.risk,
            r.preview.clone(),
        ),
        UserActionRequest::Elicitation(r) => MessageContent::action_required_elicitation(
            id,
            r.message.clone(),
            r.requested_schema.clone(),
        ),
        // Key names and labels only. Whatever the user types goes from the
        // trusted surface to the keyring; this card is the ask, never the answer.
        UserActionRequest::Secrets(r) => MessageContent::action_required_secrets(
            id,
            r.prompt.clone(),
            r.keys.clone(),
            r.destination.clone(),
        ),
    };
    // `user_only` for the same reason `handle_approval_tool_requests` marks its
    // card: a decision prompt is for the person, and a model that read one would
    // be reading a question it cannot answer. It is belt-and-braces here — the
    // drain does not persist an ephemeral card and never pushes one into the
    // conversation — but the flag is what keeps that true if either changes.
    match request {
        UserActionRequest::Elicitation(_) => Message::assistant().with_content(content),
        // An elicitation is deliberately NOT user-only: its answer is part of
        // the conversation, and the model that raised it has to see both.
        _ => Message::assistant().with_content(content).user_only(),
    }
}

/// Whether `message` is a decision prompt rather than a record.
///
/// An approval or credential card exists only while somebody has to answer it.
/// Persisting one means reopening the session shows a live-looking dialog for a
/// call that finished long ago — and clicking it posts a decision to a request
/// id that no longer exists. `handle_approval_tool_requests` has always yielded
/// its card without persisting; this is the same rule, applied where the drain
/// can see it.
///
/// An **elicitation** is deliberately not ephemeral: its answer is part of the
/// conversation, and the `ElicitationResponse` row references this message's id.
pub fn is_ephemeral_card(message: &Message) -> bool {
    use crate::conversation::message::ActionRequiredData;
    message.content.iter().any(|content| {
        matches!(
            content,
            MessageContent::ActionRequired(action)
                if matches!(
                    action.data,
                    ActionRequiredData::ToolConfirmation { .. }
                        | ActionRequiredData::SecretRequest { .. }
                )
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approval(tool: &str) -> UserActionRequest {
        UserActionRequest::ToolApproval(ToolApprovalRequest {
            tool_name: tool.to_string(),
            arguments: serde_json::Map::new(),
            prompt: None,
            risk: None,
            preview: None,
        })
    }

    fn secrets() -> UserActionRequest {
        UserActionRequest::Secrets(SecretsRequest {
            prompt: "SPOKEAgent needs its passcode".to_string(),
            keys: vec![SecretKeyRequest {
                key: "SPOKEAGENT_PASSCODE".to_string(),
                label: "Passcode".to_string(),
                description: None,
                required: true,
            }],
            destination: SecretDestination::Keyring,
        })
    }

    #[tokio::test]
    async fn an_approval_unparks_with_the_decision() {
        let registry = Arc::new(PendingUserActions::default());
        let parked = registry.park(Some("sess-1"), None, approval("developer__shell"));
        let id = parked.id().to_string();
        assert_eq!(
            registry.resolve_in_session(
                "sess-1",
                &id,
                UserActionOutcome::Approved {
                    permission: Permission::AllowOnce
                }
            ),
            ResolveOutcome::Delivered
        );
        assert_eq!(
            parked.wait(Duration::from_secs(5), None).await,
            UserActionOutcome::Approved {
                permission: Permission::AllowOnce
            }
        );
    }

    #[tokio::test]
    async fn a_foreign_session_is_unknown_and_leaves_the_waiter_parked() {
        let registry = Arc::new(PendingUserActions::default());
        let parked = registry.park(Some("sess-owner"), None, approval("developer__shell"));
        let id = parked.id().to_string();

        assert_eq!(
            registry.resolve_in_session(
                "sess-foreign",
                &id,
                UserActionOutcome::Approved {
                    permission: Permission::AllowOnce,
                },
            ),
            ResolveOutcome::Unknown
        );
        assert!(
            registry.is_pending(&id),
            "the real waiter must remain parked"
        );

        assert_eq!(
            registry.resolve_in_session(
                "sess-owner",
                &id,
                UserActionOutcome::Denied {
                    permission: Permission::DenyOnce,
                },
            ),
            ResolveOutcome::Delivered
        );
        assert_eq!(
            parked.wait(Duration::from_secs(5), None).await,
            UserActionOutcome::Denied {
                permission: Permission::DenyOnce,
            }
        );
    }

    #[tokio::test]
    async fn the_trusted_sessionless_resolver_refuses_non_secret_requests() {
        let registry = Arc::new(PendingUserActions::default());
        let parked = registry.park(Some("sess-owner"), None, approval("developer__shell"));
        let id = parked.id().to_string();

        assert_eq!(
            registry.resolve_trusted_sessionless_secret(
                &id,
                UserActionOutcome::Approved {
                    permission: Permission::AllowOnce,
                },
            ),
            ResolveOutcome::Unknown
        );
        assert!(registry.is_pending(&id), "the approval must remain parked");

        assert_eq!(
            registry.resolve_in_session(
                "sess-owner",
                &id,
                UserActionOutcome::Denied {
                    permission: Permission::DenyOnce,
                },
            ),
            ResolveOutcome::Delivered
        );
        let _ = parked.wait(Duration::from_secs(5), None).await;
    }

    /// The #107 property that made the bug possible: two bridged calls are in
    /// flight at once (both CLIs issue parallel `tools/call`), and a decision
    /// for one must never release the other.
    #[tokio::test]
    async fn concurrent_requests_cannot_resolve_each_other() {
        let registry = Arc::new(PendingUserActions::default());
        let a = registry.park(Some("sess-1"), None, approval("developer__shell"));
        let b = registry.park(Some("sess-1"), None, approval("developer__text_editor"));
        let b_id = b.id().to_string();
        assert_ne!(a.id(), b.id());

        registry.resolve_in_session(
            "sess-1",
            &b_id,
            UserActionOutcome::Denied {
                permission: Permission::DenyOnce,
            },
        );

        assert_eq!(
            b.wait(Duration::from_secs(5), None).await,
            UserActionOutcome::Denied {
                permission: Permission::DenyOnce
            }
        );
        // A is untouched and still parked: only its own TTL releases it.
        assert!(registry.is_pending(a.id()));
        assert_eq!(
            a.wait(Duration::from_millis(50), None).await,
            UserActionOutcome::TimedOut
        );
    }

    #[tokio::test]
    async fn a_timeout_is_an_outcome_not_an_error() {
        let registry = Arc::new(PendingUserActions::default());
        let parked = registry.park(Some("sess-1"), None, approval("developer__shell"));
        assert_eq!(
            parked.wait(Duration::from_millis(30), None).await,
            UserActionOutcome::TimedOut
        );
    }

    #[tokio::test]
    async fn the_turns_cancel_token_releases_the_park() {
        let registry = Arc::new(PendingUserActions::default());
        let parked = registry.park(Some("sess-1"), None, approval("developer__shell"));
        let token = CancellationToken::new();
        token.cancel();
        assert_eq!(
            // A TTL far longer than the test could tolerate: if the token were
            // not honoured this would hang rather than fail.
            parked.wait(Duration::from_secs(600), Some(&token)).await,
            UserActionOutcome::Cancelled
        );
    }

    /// A bridge lease drop must release every call parked under that turn —
    /// otherwise a panicking turn leaves the child blocked on an HTTP response
    /// for the full TTL.
    #[tokio::test]
    async fn cancel_owner_releases_only_that_turns_parks() {
        let registry = Arc::new(PendingUserActions::default());
        let mine = registry.park(Some("s"), Some("nonce-a"), approval("t1"));
        let theirs = registry.park(Some("s"), Some("nonce-b"), approval("t2"));
        assert_eq!(registry.cancel_owner("nonce-a"), 1);
        assert_eq!(
            mine.wait(Duration::from_secs(5), None).await,
            UserActionOutcome::Cancelled
        );
        assert!(registry.is_pending(theirs.id()));
    }

    #[tokio::test]
    async fn cancel_session_releases_only_that_sessions_parks() {
        let registry = Arc::new(PendingUserActions::default());
        let a = registry.park(Some("sess-a"), None, approval("t1"));
        let b = registry.park(Some("sess-b"), None, approval("t2"));
        assert_eq!(registry.cancel_session("sess-a"), 1);
        assert_eq!(
            a.wait(Duration::from_secs(5), None).await,
            UserActionOutcome::Cancelled
        );
        assert!(registry.is_pending(b.id()));
    }

    /// The secret-safety guarantee, enforced rather than documented: a route
    /// that answered a credential card with the value would put it on the same
    /// path every tool result takes. `Provided` is refused, the caller stays
    /// parked, and the surface's bug is visible instead of silent.
    #[tokio::test]
    async fn a_secrets_request_refuses_a_value_bearing_outcome() {
        let registry = Arc::new(PendingUserActions::default());
        let parked = registry.park(Some("sess-1"), None, secrets());
        let id = parked.id().to_string();
        assert_eq!(
            registry.resolve_in_session(
                "sess-1",
                &id,
                UserActionOutcome::Provided {
                    data: serde_json::json!({ "SPOKEAGENT_PASSCODE": "hunter2" })
                }
            ),
            ResolveOutcome::Rejected
        );
        assert!(registry.is_pending(&id), "the caller must stay parked");

        assert_eq!(
            registry.resolve_in_session(
                "sess-1",
                &id,
                UserActionOutcome::SecretsConfigured {
                    configured_keys: vec!["SPOKEAGENT_PASSCODE".to_string()]
                }
            ),
            ResolveOutcome::Delivered
        );
        assert_eq!(
            parked.wait(Duration::from_secs(5), None).await,
            UserActionOutcome::SecretsConfigured {
                configured_keys: vec!["SPOKEAGENT_PASSCODE".to_string()]
            }
        );
    }

    /// A secrets card must be publishable without the value existing yet, and
    /// the card itself must carry no field a value could sit in.
    #[test]
    fn a_secrets_card_carries_names_only() {
        let card = request_message("req-1", &secrets());
        let json = serde_json::to_string(&card).expect("serialisable");
        assert!(
            json.contains("SPOKEAGENT_PASSCODE"),
            "the key name is shown"
        );
        assert!(
            !json.contains("hunter2"),
            "nothing in the card can carry a value"
        );
    }

    #[tokio::test]
    async fn an_approval_refuses_an_elicitation_answer() {
        let registry = Arc::new(PendingUserActions::default());
        let parked = registry.park(Some("sess-1"), None, approval("developer__shell"));
        assert_eq!(
            registry.resolve_in_session(
                "sess-1",
                parked.id(),
                UserActionOutcome::Provided {
                    data: serde_json::json!({})
                }
            ),
            ResolveOutcome::Rejected
        );
    }

    /// Approval and credential cards must not be written into history;
    /// elicitations must, because their answer row references them.
    #[test]
    fn only_decision_prompts_are_ephemeral() {
        assert!(is_ephemeral_card(&request_message(
            "1",
            &approval("developer__shell")
        )));
        assert!(is_ephemeral_card(&request_message("2", &secrets())));
        assert!(!is_ephemeral_card(&request_message(
            "3",
            &UserActionRequest::Elicitation(ElicitationRequest {
                message: "Which cohort?".to_string(),
                requested_schema: serde_json::json!({}),
            })
        )));
        assert!(!is_ephemeral_card(&Message::assistant().with_text("hi")));
    }

    /// A chat-less run's card must be UNSCOPED, not queued under a session
    /// named "". Every such run in the process would otherwise share one queue,
    /// which is #40's cross-session leak with a different key.
    #[tokio::test]
    async fn an_unscoped_park_is_deliverable_to_any_loop() {
        use crate::action_required_manager::ActionRequiredManager;
        let registry = Arc::clone(PendingUserActions::global());
        let parked = registry.park(None, None, approval("developer__shell"));
        let id = parked.id().to_string();
        // Any session's loop can drain it, which is what "unscoped" means.
        let drained = ActionRequiredManager::global().drain_requests("some-other-session");
        assert!(
            drained.iter().any(|m| m.content.iter().any(|c| {
                matches!(
                    c,
                    MessageContent::ActionRequired(a)
                        if matches!(
                            &a.data,
                            crate::conversation::message::ActionRequiredData::ToolConfirmation {
                                id: card_id, ..
                            } if *card_id == id
                        )
                )
            })),
            "an unscoped card must reach a loop that is not its own session's"
        );
        assert_eq!(
            registry.resolve_in_session(
                "foreign-session",
                &id,
                UserActionOutcome::Approved {
                    permission: Permission::AllowOnce,
                },
            ),
            ResolveOutcome::Unknown
        );
        assert!(registry.is_pending(&id));
        assert_eq!(
            registry.resolve_in_session(
                "some-other-session",
                &id,
                UserActionOutcome::Approved {
                    permission: Permission::AllowOnce,
                },
            ),
            ResolveOutcome::Delivered
        );
        let _ = parked.wait(Duration::from_secs(5), None).await;
    }

    #[tokio::test]
    async fn a_decision_for_an_unknown_id_is_dropped() {
        let registry = Arc::new(PendingUserActions::default());
        assert_eq!(
            registry.resolve_in_session("s", "no-such-request", UserActionOutcome::Cancelled),
            ResolveOutcome::Unknown
        );
    }

    /// A double-click is a no-op, not a second decision aimed at whatever is
    /// parked now.
    #[tokio::test]
    async fn a_duplicate_decision_is_unknown() {
        let registry = Arc::new(PendingUserActions::default());
        let parked = registry.park(Some("s"), None, approval("t"));
        let id = parked.id().to_string();
        assert_eq!(
            registry.resolve_in_session(
                "s",
                &id,
                UserActionOutcome::Approved {
                    permission: Permission::AllowOnce
                }
            ),
            ResolveOutcome::Delivered
        );
        assert_eq!(
            registry.resolve_in_session(
                "s",
                &id,
                UserActionOutcome::Approved {
                    permission: Permission::AlwaysAllow
                }
            ),
            ResolveOutcome::Unknown
        );
    }

    /// Dropping the handle without awaiting deregisters the request: a decision
    /// arriving afterwards has nowhere to land and says so.
    #[tokio::test]
    async fn dropping_the_handle_deregisters_the_request() {
        let registry = Arc::new(PendingUserActions::default());
        let id = {
            let parked = registry.park(Some("s"), None, approval("t"));
            assert!(registry.is_pending(parked.id()));
            parked.id().to_string()
        };
        assert!(!registry.is_pending(&id));
    }

    /// The refusal text must never tell a model that a chat message can approve
    /// something — on the bridged path the request id is gone by the time the
    /// model reads it (#107).
    #[test]
    fn no_outcome_claims_a_chat_message_can_approve() {
        for outcome in [
            UserActionOutcome::Denied {
                permission: Permission::DenyOnce,
            },
            UserActionOutcome::Cancelled,
            UserActionOutcome::TimedOut,
            UserActionOutcome::Failed {
                reason: "no surface".to_string(),
            },
        ] {
            let detail = outcome.refusal_detail();
            assert!(
                !detail.contains("approve it") && !detail.contains("let them"),
                "{detail:?} invites an answer that cannot land"
            );
        }
    }
}
