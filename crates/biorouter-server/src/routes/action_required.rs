use crate::state::AppState;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use biorouter::agents::approval_relay::{self, ResolveOutcome};
use biorouter::agents::ConfirmationOutcome;
use biorouter::extension_install::{cancel_credentials, submit_credentials, SubmitOutcome};
use biorouter::permission::permission_confirmation::PrincipalType;
use biorouter::permission::{Permission, PermissionConfirmation};
use biorouter_server::auth::{user_action_proof, UserActionProof};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
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

/// Answering a credential card (#117).
///
/// ⚠ **This request body carries secrets and nothing else in the codebase does.**
/// The values reach `submit_credentials`, which writes them to the OS credential
/// store and drops them. Do not add a field to the response that could carry one
/// back, do not add `#[derive(Debug)]` (see the hand-written impl below), and do
/// not log the body.
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmitSecretsRequest {
    /// The card id, as published on the `secretRequest` message.
    id: String,
    /// What the user typed, keyed by the card's key names.
    #[serde(default)]
    values: HashMap<String, String>,
    /// The user dismissed the dialog. `values` is ignored when this is set.
    #[serde(default)]
    cancelled: bool,
}

/// Redacting, deliberately.
///
/// `Debug` is derived on every other request type here, and a derive on this one
/// would put a passcode into any `tracing` line, panic message or test failure
/// that happened to format the request. Naming the *keys* keeps the type
/// debuggable without that ever being possible.
impl std::fmt::Debug for SubmitSecretsRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut keys: Vec<&str> = self.values.keys().map(String::as_str).collect();
        keys.sort_unstable();
        f.debug_struct("SubmitSecretsRequest")
            .field("id", &self.id)
            .field("keys", &keys)
            .field("cancelled", &self.cancelled)
            .finish()
    }
}

/// Store the credentials an extension install is parked on, and release it.
///
/// The response reports **names only**: `configuredKeys` on success, `missing`
/// when a required field came back empty. There is nowhere in it a value can
/// sit, which is what lets the parked install — and therefore the model — be
/// told the truth about what happened without being told what it was.
///
/// Requires the DR-16 proof-of-user header. The model reaches this daemon over
/// the same HTTP with the same secret key, so without the proof it could satisfy
/// its own credential card with a value it invented and drive the install past
/// the one step that exists to involve a person.
#[utoipa::path(
    post,
    path = "/action-required/secrets",
    request_body = SubmitSecretsRequest,
    responses(
        (status = 200, description = "Names of the keys configured, or which required ones are still missing", body = Value),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 403, description = "The request carried no proof it came from the user"),
    )
)]
pub async fn submit_secrets(
    State(_state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<SubmitSecretsRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match user_action_proof(&headers) {
        UserActionProof::Proven => {}
        UserActionProof::Unproven => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "status": "refused",
                    "error": UNPROVEN_REFUSAL,
                })),
            ))
        }
        UserActionProof::NoKeyInstalled => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "status": "refused",
                    "error": NO_KEY_REFUSAL,
                })),
            ))
        }
    }

    if request.cancelled {
        let delivered = cancel_credentials(&request.id);
        return Ok(Json(serde_json::json!({
            "status": if delivered { "cancelled" } else { "unknown" },
        })));
    }

    Ok(Json(
        match submit_credentials(&request.id, request.values) {
            SubmitOutcome::Configured { configured_keys } => serde_json::json!({
                "status": "configured",
                "configuredKeys": configured_keys,
            }),
            SubmitOutcome::Incomplete { missing } => serde_json::json!({
                "status": "incomplete",
                "missing": missing,
            }),
            SubmitOutcome::Unknown => serde_json::json!({ "status": "unknown" }),
            SubmitOutcome::Failed { reason } => serde_json::json!({
                "status": "failed",
                "reason": reason,
            }),
        },
    ))
}

/// ⚠ Written for a MODEL to read, in the register `SESSION_OUT_OF_REACH` uses:
/// it forecloses a retry and never suggests that typing the value into the chat
/// could work. A refusal that invited one would be asking the user to do the
/// exact thing this whole feature exists to stop.
pub const UNPROVEN_REFUSAL: &str =
    "Credentials can only be submitted by the person at the keyboard, \
     through Biorouter's own dialog. Do not retry, and do not ask for the value in chat — \
     a value in a chat message cannot configure anything and would expose it. \
     Tell the user the dialog is waiting for them.";

pub const NO_KEY_REFUSAL: &str =
    "This Biorouter daemon was started without a way to tell a person from a model, \
     so it cannot accept credentials over HTTP. Configure them at a terminal with \
     `biorouter extension install <bundle>`, which prompts with echo off.";

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/action-required/tool-confirmation",
            post(confirm_tool_action),
        )
        .route("/action-required/secrets", post(submit_secrets))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hand-written `Debug` impl on [`SubmitSecretsRequest`] is the only
    /// thing keeping a passcode out of a `tracing` line, a panic message, or a
    /// test failure that happens to format the request. Because it is written
    /// rather than derived, a later `#[derive(Debug)]` would restore the leak
    /// while every other test in this file kept passing -- the logs surface is
    /// the one credential surface with no other assertion on it.
    #[test]
    fn the_debug_impl_names_the_keys_and_never_the_values() {
        let req: SubmitSecretsRequest = serde_json::from_value(serde_json::json!({
            "id": "card-1",
            "values": {
                "SPOKEAGENT_PASSCODE": "hunter2-the-actual-secret",
                "UCSF_TOKEN": "second-secret-value",
            },
            "cancelled": false,
        }))
        .unwrap();

        for rendered in [format!("{req:?}"), format!("{req:#?}")] {
            assert!(
                !rendered.contains("hunter2-the-actual-secret"),
                "a credential value reached a Debug rendering: {rendered}"
            );
            assert!(
                !rendered.contains("second-secret-value"),
                "a credential value reached a Debug rendering: {rendered}"
            );
            assert!(
                rendered.contains("SPOKEAGENT_PASSCODE"),
                "the key names are what make this type debuggable: {rendered}"
            );
            assert!(rendered.contains("card-1"), "the card id should survive: {rendered}");
        }
    }

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

    /// Issue #117. The credential card, over the wire.
    ///
    /// These are the assertions the feature's security argument rests on, and
    /// none of them can be made from inside `biorouter`: only here do a request
    /// body, an auth header and a response body exist at once.
    mod secrets_tests {
        use super::*;
        use crate::routes::session::diverge_tests::{
            install_test_user_action_key, TEST_USER_ACTION_KEY,
        };
        use axum::{body::Body, http::Request};
        use biorouter::conversation::message::SecretDestination;
        use biorouter::extension_install::{park_credentials, BrxtEnvVar, CredentialSpec};
        use biorouter::pending_user_action::PendingUserActions;
        use serial_test::serial;
        use tower::ServiceExt;

        fn var(key: &str, required: bool, secret: bool) -> BrxtEnvVar {
            BrxtEnvVar {
                key: key.to_string(),
                required,
                auto_propagate: false,
                default: None,
                description: String::new(),
                secret,
            }
        }

        fn spec(vars: Vec<BrxtEnvVar>) -> CredentialSpec {
            CredentialSpec {
                destination: SecretDestination::ExtensionEnv {
                    extension_name: "spokeagent".to_string(),
                },
                vars,
            }
        }

        fn post(body: Value, user_action: Option<&str>) -> Request<Body> {
            let mut builder = Request::builder()
                .uri("/action-required/secrets")
                .method("POST")
                .header("content-type", "application/json")
                .header("x-secret-key", "test-secret");
            if let Some(key) = user_action {
                builder = builder.header("X-User-Action", key);
            }
            builder.body(Body::from(body.to_string())).unwrap()
        }

        async fn body_of(response: axum::response::Response) -> Value {
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            serde_json::from_slice(&bytes).unwrap()
        }

        /// The whole point, stated as an assertion: what the user typed goes to
        /// the credential store, and the answer carries the key's NAME.
        #[tokio::test(flavor = "multi_thread")]
        #[serial]
        async fn a_submitted_value_is_never_echoed_back() {
            install_test_user_action_key();
            let app = routes(AppState::new().await.unwrap());

            let parked = park_credentials(
                Some("s-echo"),
                None,
                "Configure".to_string(),
                // Non-secret so the test writes nothing to the machine's
                // credential store; the response shape is identical either way.
                spec(vec![var("OMOP_HOST", true, false)]),
            );
            let id = parked.id().to_string();

            let response = app
                .clone()
                .oneshot(post(
                    serde_json::json!({
                        "id": id,
                        "values": { "OMOP_HOST": "https://omop.internal.example" },
                    }),
                    Some(TEST_USER_ACTION_KEY),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let body = body_of(response).await;
            assert_eq!(body["status"], "configured");
            assert_eq!(body["configuredKeys"], serde_json::json!(["OMOP_HOST"]));
            let rendered = body.to_string();
            assert!(
                !rendered.contains("omop.internal.example"),
                "the response echoed the submitted value back: {rendered}"
            );

            let (outcome, settings) = parked.wait(std::time::Duration::from_secs(5), None).await;
            assert!(outcome.is_allowed());
            // The non-secret setting reaches the install out of band — never
            // through the conversation transport, and never through the model.
            assert_eq!(
                settings.get("OMOP_HOST").map(String::as_str),
                Some("https://omop.internal.example")
            );
        }

        /// DR-16. The model reaches this daemon over the same HTTP with the same
        /// secret key, so without the proof-of-user header it could satisfy its
        /// own credential card and drive the install past the one step that
        /// exists to involve a person.
        #[tokio::test(flavor = "multi_thread")]
        #[serial]
        async fn a_request_without_the_proof_of_user_header_is_refused() {
            install_test_user_action_key();
            let app = routes(AppState::new().await.unwrap());

            let parked = park_credentials(
                Some("s-unproven"),
                None,
                "Configure".to_string(),
                spec(vec![var("OMOP_HOST", true, false)]),
            );
            let id = parked.id().to_string();

            let response = app
                .clone()
                .oneshot(post(
                    serde_json::json!({ "id": id, "values": { "OMOP_HOST": "x" } }),
                    None,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);

            let error = body_of(response).await["error"]
                .as_str()
                .unwrap()
                .to_string();
            // ⚠ The refusal is read by a MODEL. It must not send it back to ask
            // the user for the value in chat, which is the exact failure #117
            // exists to end.
            let lowered = error.to_lowercase();
            assert!(
                !lowered.contains("ask the user for the value") && !lowered.contains("paste"),
                "the refusal invited a chat answer: {error}"
            );
            assert!(lowered.contains("do not retry"));

            assert!(
                PendingUserActions::global().is_pending(&id),
                "a refused submission must leave the install parked"
            );
            drop(parked);
        }

        /// BR-62's property on the credential path: two installs in flight
        /// cannot answer each other, and a replayed answer lands nowhere.
        #[tokio::test(flavor = "multi_thread")]
        #[serial]
        async fn one_dialog_cannot_satisfy_another_installs_request() {
            install_test_user_action_key();
            let app = routes(AppState::new().await.unwrap());

            let first = park_credentials(
                Some("s-one"),
                None,
                "One".to_string(),
                spec(vec![var("ONE_HOST", true, false)]),
            );
            let second = park_credentials(
                Some("s-two"),
                None,
                "Two".to_string(),
                spec(vec![var("TWO_HOST", true, false)]),
            );
            let (id_one, id_two) = (first.id().to_string(), second.id().to_string());

            let response = app
                .clone()
                .oneshot(post(
                    serde_json::json!({ "id": id_one, "values": { "ONE_HOST": "a" } }),
                    Some(TEST_USER_ACTION_KEY),
                ))
                .await
                .unwrap();
            assert_eq!(body_of(response).await["status"], "configured");
            assert!(
                PendingUserActions::global().is_pending(&id_two),
                "answering one install released another"
            );

            // A replay of the same id is a no-op, not a second grant, and it
            // does not fall through onto whatever is parked now.
            let response = app
                .clone()
                .oneshot(post(
                    serde_json::json!({ "id": id_one, "values": { "ONE_HOST": "b" } }),
                    Some(TEST_USER_ACTION_KEY),
                ))
                .await
                .unwrap();
            assert_eq!(body_of(response).await["status"], "unknown");
            assert!(PendingUserActions::global().is_pending(&id_two));

            let (outcome, _) = first.wait(std::time::Duration::from_secs(5), None).await;
            assert!(outcome.is_allowed());
            drop(second);
        }

        /// A typo is not a rollback: an empty required field leaves the dialog
        /// open and the install parked, and says which field.
        #[tokio::test(flavor = "multi_thread")]
        #[serial]
        async fn an_empty_required_field_leaves_the_dialog_open() {
            install_test_user_action_key();
            let app = routes(AppState::new().await.unwrap());

            let parked = park_credentials(
                Some("s-empty"),
                None,
                "Configure".to_string(),
                spec(vec![var("OMOP_HOST", true, false)]),
            );
            let id = parked.id().to_string();

            let response = app
                .clone()
                .oneshot(post(
                    serde_json::json!({ "id": id, "values": { "OMOP_HOST": "   " } }),
                    Some(TEST_USER_ACTION_KEY),
                ))
                .await
                .unwrap();
            let body = body_of(response).await;
            assert_eq!(body["status"], "incomplete");
            assert_eq!(body["missing"], serde_json::json!(["OMOP_HOST"]));
            assert!(PendingUserActions::global().is_pending(&id));
            drop(parked);
        }

        /// Cancel is a first-class result, and it releases the install so it can
        /// roll back rather than leaving it parked until the TTL.
        #[tokio::test(flavor = "multi_thread")]
        #[serial]
        async fn cancelling_releases_the_install() {
            install_test_user_action_key();
            let app = routes(AppState::new().await.unwrap());

            let parked = park_credentials(
                Some("s-cancel"),
                None,
                "Configure".to_string(),
                spec(vec![var("OMOP_HOST", true, false)]),
            );
            let id = parked.id().to_string();

            let response = app
                .clone()
                .oneshot(post(
                    serde_json::json!({ "id": id, "cancelled": true }),
                    Some(TEST_USER_ACTION_KEY),
                ))
                .await
                .unwrap();
            assert_eq!(body_of(response).await["status"], "cancelled");

            let (outcome, _) = parked.wait(std::time::Duration::from_secs(5), None).await;
            assert!(!outcome.is_allowed());
        }

        /// The redacting `Debug` impl, asserted rather than assumed: a derive
        /// here would put a passcode into any log line that formatted the body.
        #[test]
        fn the_request_debug_impl_names_keys_and_never_values() {
            let request: SubmitSecretsRequest = serde_json::from_value(serde_json::json!({
                "id": "card-1",
                "values": { "SPOKEAGENT_PASSCODE": "hunter2" },
            }))
            .unwrap();
            let rendered = format!("{request:?}");
            assert!(rendered.contains("SPOKEAGENT_PASSCODE"));
            assert!(
                !rendered.contains("hunter2"),
                "the request Debug impl leaked a value: {rendered}"
            );
        }
    }
}
