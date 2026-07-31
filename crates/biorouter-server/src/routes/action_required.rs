use crate::state::AppState;
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use biorouter::agents::approval_relay::{self, ResolveOutcome};
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
    let permission = match request.action.as_str() {
        "always_allow" => Permission::AlwaysAllow,
        "allow_once" => Permission::AllowOnce,
        "deny" => Permission::DenyOnce,
        _ => Permission::DenyOnce,
    };

    // BR-71 Task 36b: two surfaces, ONE pending ask. The escalation card in the
    // parent's chat carries the origin's tool request id, so a decision posted
    // from either session resolves the same relay entry — and the OTHER surface
    // is dismissed rather than left pending.
    if let Some(ask) = approval_relay::lookup(&request.id, &request.session_id) {
        // ⚠ Resolve the ORIGIN's agent handle BEFORE `resolve` marks the ask
        // decided. `resolve` is a one-way door: it stamps the decision, clears
        // every surface and writes the tree memo. Doing that first and then
        // `?`-ing out of a failed lookup would consume the ask without ever
        // delivering it — the child's prompt stays parked until it times out,
        // both cards are unanswerable (every retry from either surface finds no
        // surface to route from, and one that did would get `already_resolved`),
        // and the memo now holds a grant for a decision that was never applied,
        // so the NEXT identical ask in that tree is auto-approved from it. The
        // fallible step therefore runs while the ask is still untouched.
        let origin = state.get_agent_for_route(ask.session_id.clone()).await?;
        return match approval_relay::resolve(&ask, permission, &request.session_id) {
            ResolveOutcome::Resolved { decision, notify } => {
                let outcome = origin
                    .handle_confirmation(
                        ask.request_id.clone(),
                        PermissionConfirmation {
                            principal_type: request.principal_type,
                            permission: decision,
                        },
                    )
                    .await;
                dismiss_on(&notify, &ask.request_id).await;
                Ok(Json(serde_json::json!({
                    "status": match outcome {
                        ConfirmationOutcome::Delivered => "delivered",
                        ConfirmationOutcome::Unknown => "unknown",
                    },
                    "dismissed": notify,
                })))
            }
            // The other surface got there first. 200 and the truth: a
            // double-click is a no-op, and the client reconciles its card from
            // the decision rather than re-posting.
            ResolveOutcome::AlreadyResolved(decision) => Ok(Json(serde_json::json!({
                "status": "already_resolved",
                "decision": decision,
            }))),
            ResolveOutcome::Unknown => Ok(Json(serde_json::json!({ "status": "unknown" }))),
        };
    }

    // Not a delegated ask: the pre-Task-36b path, unchanged.
    let agent = state.get_agent_for_route(request.session_id).await?;
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

/// A-5: tell every other surface the ask is answered, so its card stops showing
/// as pending. Best-effort — the relay is the truth, and a client that misses
/// the frame learns on its next POST (`already_resolved`).
///
/// ⚠ **The renderer does not handle `resolve_confirmation` yet.** The desktop
/// command union (`workspaceCommandRegistry.ts`) covers `open_tab` /
/// `activate_tab` / `close_tab` / `open_window` / `notify` / `annotate_tab`, so
/// today this frame is dropped and the second card keeps *rendering* as pending
/// until the user clicks it, at which point the `already_resolved` response
/// reconciles it. The wire contract below is the finished half; adding the
/// `resolve_confirmation` case to that registry is Task 25/37's, and until it
/// lands "approve once, both cards clear on their own" is only true for the
/// surface that was clicked. Nothing here needs to change when it does.
async fn dismiss_on(session_ids: &[String], request_id: &str) {
    let Some(services) = biorouter::workspace_services::get() else {
        return;
    };
    if !services.gui_attached() {
        return;
    }
    for session_id in session_ids {
        let _ = services
            .gui_command(
                serde_json::json!({
                    "type": "workspace",
                    "cmd": "resolve_confirmation",
                    "session_id": session_id,
                    "request_id": request_id,
                }),
                false,
            )
            .await;
    }
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

        fn post(action: &str, id: &str, session_id: &str) -> Request<Body> {
            Request::builder()
                .uri("/action-required/tool-confirmation")
                .method("POST")
                .header("content-type", "application/json")
                .header("x-secret-key", "test-secret")
                .body(Body::from(
                    serde_json::to_string(&ConfirmToolActionRequest {
                        id: id.to_string(),
                        principal_type: PrincipalType::Tool,
                        action: action.to_string(),
                        session_id: session_id.to_string(),
                    })
                    .unwrap(),
                ))
                .unwrap()
        }

        async fn body_json(response: axum::response::Response) -> Value {
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            serde_json::from_slice(&bytes).unwrap()
        }

        /// BR-71 Task 36b, A-4/A-5 **on the wire**. Everything else about the
        /// relay is unit-tested inside `biorouter`; this is the only test that
        /// proves the endpoint consults it at all. Without it the handler could
        /// ignore the relay entirely — a decision posted from the escalation
        /// surface would fall through to `get_agent_for_route(root)`, find no
        /// prompt parked there, report `unknown`, and leave the child's turn
        /// waiting out its timeout — and every unit test would still pass.
        #[tokio::test(flavor = "multi_thread")]
        async fn a_decision_from_the_escalation_surface_resolves_the_origins_ask_once() {
            let state = AppState::new().await.unwrap();
            let app = routes(state);

            let tool_request = biorouter::conversation::message::ToolRequest {
                id: "br71-call-1".to_string(),
                tool_call: Ok(rmcp::model::CallToolRequestParams {
                    meta: None,
                    name: "acme__widget".into(),
                    arguments: Some(serde_json::Map::new()),
                    task: None,
                }),
                metadata: None,
                tool_meta: None,
            };
            let origin = approval_relay::AskId {
                session_id: "br71-child".to_string(),
                request_id: "br71-call-1".to_string(),
            };
            approval_relay::register(
                origin.clone(),
                approval_relay::AskKey::for_request(&tool_request).unwrap(),
                approval_relay::AskClass::Delegable,
                "br71-root".to_string(),
                "/br71/work".to_string(),
            );
            approval_relay::add_surface(&origin, "br71-child");
            approval_relay::add_surface(&origin, "br71-root");

            // The user clicks Allow in the ROOT's chat — a session that is NOT
            // the one whose agent is parked on this request id.
            let response = app
                .clone()
                .oneshot(post("allow_once", "br71-call-1", "br71-root"))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = body_json(response).await;
            assert_eq!(
                body["dismissed"],
                serde_json::json!(["br71-child"]),
                "the origin's own card must be named for dismissal, or it stays pending forever"
            );

            // Clicking again — in either place — is a no-op, not a second grant.
            let response = app
                .clone()
                .oneshot(post("deny", "br71-call-1", "br71-child"))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(body_json(response).await["status"], "already_resolved");

            approval_relay::forget(&origin);
        }
    }
}
