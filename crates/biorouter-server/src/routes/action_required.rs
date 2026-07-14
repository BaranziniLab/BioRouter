use crate::state::AppState;
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use biorouter::agents::ConfirmationOutcome;
use biorouter::permission::permission_confirmation::PrincipalType;
use biorouter::permission::{Permission, PermissionConfirmation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmToolActionRequest {
    id: String,
    #[serde(default = "default_principal_type")]
    principal_type: PrincipalType,
    action: String,
    session_id: String,
}

fn default_principal_type() -> PrincipalType {
    PrincipalType::Tool
}

/// Deliver a tool-permission decision to the prompt that is waiting for it.
///
/// **Idempotent** (BR-62). The decision is routed by tool request id to that
/// prompt's own channel, so a decision for an id nobody is waiting on — a
/// double-clicked Allow, a card the user answered after the prompt expired or the
/// turn was cancelled, a stale client replaying an old confirmation — is dropped
/// rather than applied to whatever tool call happens to be pending now. (Before
/// BR-62 confirmations went to a single per-agent channel, so a late "allow"
/// really could approve an unrelated later tool call.)
///
/// Both outcomes are a 200: a duplicate click is a no-op, not a failure. The
/// `status` field reports which happened — `delivered` when a live prompt took
/// the decision, `unknown` when nothing was waiting on that id.
#[utoipa::path(
    post,
    path = "/action-required/tool-confirmation",
    request_body = ConfirmToolActionRequest,
    responses(
        (status = 200, description = "Decision processed; `status` is `delivered` or `unknown`", body = Value),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn confirm_tool_action(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ConfirmToolActionRequest>,
) -> Result<Json<Value>, StatusCode> {
    let agent = state.get_agent_for_route(request.session_id).await?;
    let permission = match request.action.as_str() {
        "always_allow" => Permission::AlwaysAllow,
        "allow_once" => Permission::AllowOnce,
        "deny" => Permission::DenyOnce,
        _ => Permission::DenyOnce,
    };

    let outcome = agent
        .handle_confirmation(
            request.id.clone(),
            PermissionConfirmation {
                principal_type: request.principal_type,
                permission,
            },
        )
        .await;

    let status = match outcome {
        ConfirmationOutcome::Delivered => "delivered",
        ConfirmationOutcome::Unknown => "unknown",
    };

    Ok(Json(serde_json::json!({ "status": status })))
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/action-required/tool-confirmation",
            post(confirm_tool_action),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod integration_tests {
        use super::*;
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;

        #[tokio::test(flavor = "multi_thread")]
        async fn test_tool_confirmation_endpoint() {
            let state = AppState::new().await.unwrap();

            let app = routes(state);

            let request = Request::builder()
                .uri("/action-required/tool-confirmation")
                .method("POST")
                .header("content-type", "application/json")
                .header("x-secret-key", "test-secret")
                .body(Body::from(
                    serde_json::to_string(&ConfirmToolActionRequest {
                        id: "test-id".to_string(),
                        principal_type: PrincipalType::Tool,
                        action: "allow_once".to_string(),
                        session_id: "test-session".to_string(),
                    })
                    .unwrap(),
                ))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();

            assert_eq!(response.status(), StatusCode::OK);
        }
    }
}
