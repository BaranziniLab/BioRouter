//! F-15 Gap A, at the route that has no person behind it.
//!
//! `POST /agent/call_tool` arrives outside the agent loop: no admitted turn, no
//! stream, nothing to draw an approval card on. A tool that parks a decision
//! there used to insert into a queue only the agent loop drains — the card
//! waited out its whole time-to-live unanswerable, and the NEXT chat turn then
//! surfaced it, as a question about something that happened minutes ago.
//!
//! ⚠ This is its own test binary because it asserts a property of the
//! **route**, and the scope is a `task_local` — a unit test that called
//! `no_human_surface()` directly would prove only that the flag can be set.

// Redirects this binary's Biorouter data/config/state dirs at a throwaway root
// before `main`, so nothing here can open the developer's real `sessions.db`.
#[path = "../src/test_sandbox.rs"]
mod test_sandbox;

use biorouter::pending_user_action::{
    PendingUserActions, ToolApprovalRequest, UserActionOutcome, UserActionRequest,
};
use std::sync::Arc;
use std::time::Duration;

fn an_approval() -> UserActionRequest {
    UserActionRequest::ToolApproval(ToolApprovalRequest {
        tool_name: "developer__shell".to_string(),
        arguments: serde_json::Map::new(),
        prompt: None,
        risk: None,
        preview: None,
        requires_user_proof: false,
    })
}

/// Stands in for whatever the dispatched tool does: it parks a decision and
/// waits. The route wraps the real `dispatch_tool_call` in exactly the scope
/// wrapped here.
async fn a_tool_that_asks(registry: &Arc<PendingUserActions>, session: &str) -> UserActionOutcome {
    let parked = registry.park(Some(session), None, an_approval());
    parked.wait(Duration::from_secs(30), None).await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tool_dispatched_with_no_human_surface_is_cancelled_not_parked() {
    let registry = Arc::new(PendingUserActions::default());
    let session = format!("call-tool-{}", std::process::id());

    let outcome =
        biorouter::user_surface::without_human_surface(a_tool_that_asks(&registry, &session)).await;

    assert!(
        matches!(outcome, UserActionOutcome::Cancelled),
        "a decision with nobody to answer it must be refused, not awaited: {outcome:?}"
    );
    // ⚠ The registration is the half that matters. `wait` returning promptly
    // would look like a fix while the entry still sat in the queue for the next
    // turn to resurrect.
    assert!(
        registry.pending_cards_for_session(&session).is_empty(),
        "a refused decision must leave no card behind for a later turn to find"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_same_tool_outside_that_scope_still_parks_normally() {
    // Catches a guard that refuses everywhere — which would silently cancel
    // every approval card in every ordinary chat turn.
    let registry = Arc::new(PendingUserActions::default());
    let session = format!("chat-{}", std::process::id());

    let asking = tokio::spawn({
        let registry = Arc::clone(&registry);
        let session = session.clone();
        async move { a_tool_that_asks(&registry, &session).await }
    });

    // The card is registered and visible to a late observer.
    let card_id = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let cards = registry.pending_cards_for_session(&session);
            if !cards.is_empty() {
                break cards;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the card must be registered");
    assert_eq!(card_id.len(), 1);

    let id = card_id
        .iter()
        .flat_map(|m| m.content.iter())
        .find_map(|c| match c {
            biorouter::conversation::message::MessageContent::ActionRequired(a) => match &a.data {
                biorouter::conversation::message::ActionRequiredData::ToolConfirmation {
                    id,
                    ..
                } => Some(id.clone()),
                _ => None,
            },
            _ => None,
        })
        .expect("the card carries its id");

    assert_eq!(
        registry.resolve_in_session(
            &session,
            &id,
            UserActionOutcome::Approved {
                permission: biorouter::permission::Permission::AllowOnce,
            },
            biorouter::pending_user_action::DecisionAuthority::unproven(),
        ),
        biorouter::pending_user_action::ResolveOutcome::Delivered
    );
    assert!(matches!(
        asking.await.unwrap(),
        UserActionOutcome::Approved { .. }
    ));
}
