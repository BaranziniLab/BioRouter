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
/// waits.
///
/// ⚠ This said "the route wraps the real `dispatch_tool_call` in exactly the
/// scope wrapped here", and that was FALSE for as long as it stood. The route
/// scoped only the dispatch, which merely BUILDS the future; the tool body ran
/// when `.result` was awaited, outside the scope. This stand-in parks inside the
/// scope, so it passed while the route hung — a fixture asserting the shape I
/// believed the route had rather than the shape it did.
///
/// `the_route_awaits_the_tool_inside_the_no_human_surface_scope` below is the
/// half that reads the route itself. Keep both: this one proves the refusal
/// works, that one proves the route is inside it.
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

/// The half the stand-in above cannot see: that the ROUTE runs the tool inside
/// the scope, not merely dispatches it inside it.
///
/// A source-shape pin, because the property is structural — `no_human_surface()`
/// is a `task_local`, so what matters is which awaits happen inside the
/// `scope()`, and no value returned by the handler records that.
///
/// Fails the shipped implementation, which read:
///
/// ```ignore
/// let tool_result = without_human_surface(dispatch_tool_call(..)).await?;
/// let result = tool_result.result.await;   // <- outside the scope
/// ```
///
/// Measured cost of that split: `installMarketplaceSkill` and
/// `importSkillPackage`, both `requires_user_proof: true`, parked a card nobody
/// could answer and never returned — 180 s with no reply, while their `dryRun`
/// siblings answered in well under 10 s.
#[test]
fn the_route_awaits_the_tool_inside_the_no_human_surface_scope() {
    let source = include_str!("../src/routes/agent.rs");

    // `split`, never byte indexing: `clippy::string_slice` is denied repo-wide,
    // and slicing a string can land inside a UTF-8 character.
    let after_open = source
        .split("without_human_surface(async {")
        .nth(1)
        .expect("call_tool must open a no-human-surface scope around an async block");
    let scoped = after_open
        .split("\n    .await;")
        .next()
        .expect("the scope's await must terminate the block");

    assert!(
        scoped.contains("dispatch_tool_call"),
        "the dispatch must happen inside the scope: {scoped}"
    );
    assert!(
        scoped.contains("tool_result.result.await"),
        "the tool must be RUN inside the scope — awaiting `.result` outside it is \
         the defect this pins, because that is where the tool body (and every \
         `park()` in it) actually executes: {scoped}"
    );

    // …and nothing awaits the result a second time outside the scope, which is
    // what the previous shape did.
    //
    // Two filters before the search, each for a real false positive: the handler
    // stops at this file's own tests, one of which QUOTES the expression as a
    // string literal to anchor its #152 assertion; and comment lines are dropped
    // because the note below the scope quotes it again while explaining it.
    let after_scope = after_open
        .split("\n    .await;")
        .nth(1)
        .expect("there must be code after the scope");
    let handler_only = after_scope.split("#[cfg(test)]").next().unwrap_or("");
    let after_code = handler_only
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !after_code.contains("tool_result.result.await"),
        "`.result` must be awaited once, inside the scope"
    );
}
