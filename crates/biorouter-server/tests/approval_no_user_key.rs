//! F-07: on a daemon that holds no proof-of-user key, an approval that requires
//! one can never be granted — by anyone, ever — and the refusal must say so in
//! words a person can act on.
//!
//! ⚠ **Its own test binary on purpose.** The installed digest is a process
//! global `OnceLock` and `routes/session.rs`'s tests install one, so inside the
//! lib test binary this state is unreachable after the first such test wins the
//! race. Nothing here installs a digest, which is exactly how `biorouter serve`
//! starts its daemon (`Stdio::null()`, SD-7).

// Redirects this binary's Biorouter data/config/state dirs at a throwaway root
// before `main`, so nothing here can open the developer's real `sessions.db`.
#[path = "../src/test_sandbox.rs"]
mod test_sandbox;

use axum::{body::Body, http::Request};
use biorouter::pending_user_action::{PendingUserActions, ToolApprovalRequest, UserActionRequest};
use biorouter_server::state::AppState;
use tower::ServiceExt;

/// ⚠ Returns the HANDLE, not just its id. Dropping a `PendingUserAction`
/// releases the park, so a helper that returned only the id would leave every
/// caller testing an approval that no longer exists — which reads as a broken
/// gate rather than a broken fixture.
fn park(
    requires_user_proof: bool,
    session_id: &str,
) -> biorouter::pending_user_action::PendingUserAction {
    PendingUserActions::global().park(
        Some(session_id),
        None,
        UserActionRequest::ToolApproval(ToolApprovalRequest {
            tool_name: "developer__shell".to_string(),
            arguments: serde_json::Map::new(),
            prompt: None,
            risk: None,
            preview: None,
            requires_user_proof,
        }),
    )
}

fn post(id: &str, session_id: &str) -> Request<Body> {
    Request::builder()
        .uri("/action-required/tool-confirmation")
        .method("POST")
        .header("content-type", "application/json")
        .header("x-secret-key", "test-secret")
        .body(Body::from(
            serde_json::json!({
                "id": id,
                "principalType": "Tool",
                "action": "allow_once",
                "sessionId": session_id,
            })
            .to_string(),
        ))
        .unwrap()
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_keyless_daemon_names_the_reason_it_can_never_approve() {
    let session = format!("keyless-{}", std::process::id());
    let parked = park(true, &session);
    let id = parked.id().to_string();
    let app = biorouter_server::routes::action_required::routes(AppState::new().await.unwrap());

    let response = app.oneshot(post(&id, &session)).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);

    let body = body_json(response).await;
    // ⚠ The reason, not just the status. A bare 403 is what sent a browser user
    // hunting for a permission that does not exist on this daemon; the whole
    // point of the fix is that the interface can tell the two refusals apart.
    assert_eq!(body["reason"], "noKeyInstalled");
    let explanation = body["error"].as_str().unwrap_or_default();
    assert!(
        explanation.contains("desktop app") || explanation.contains("Biorouter CLI"),
        "the refusal must point somewhere that works: {explanation}"
    );
    assert!(
        PendingUserActions::global().is_pending(&id),
        "a refused post must leave the approval parked for a surface that can answer it"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_ordinary_approval_is_unaffected_by_the_missing_key() {
    // ⚠ The gate must be narrow. Most approvals do not require proof, and a
    // keyless daemon that refused all of them would be broken, not safer.
    let session = format!("keyless-ordinary-{}", std::process::id());
    let parked = park(false, &session);
    let id = parked.id().to_string();
    let app = biorouter_server::routes::action_required::routes(AppState::new().await.unwrap());

    let response = app.oneshot(post(&id, &session)).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(body_json(response).await["status"], "delivered");
}
