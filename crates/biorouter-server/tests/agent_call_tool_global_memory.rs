//! `POST /agent/call_tool` against the machine-wide memory store
//! (issue #63 review, finding 3).
//!
//! The route hands a tool call straight to `ExtensionManager::dispatch_tool_call`
//! — no agent loop, so no `ToolInspector`, so no consent gate. It is one of the
//! two production routes that reach the memory tools without anything able to
//! ask the user, and it was wide open for all four operations.
//!
//! The refusal is decided before the session's agent is resolved, which is what
//! lets this test drive the real handler without standing up an agent: a
//! decision that needed one could be skipped by arriving without one.

// Redirects this binary's Biorouter data/config/state dirs at a throwaway root
// before `main`, so nothing here can open the developer's real `sessions.db`.
#[path = "../src/test_sandbox.rs"]
mod test_sandbox;

use axum::{body::Body, http::Request, http::StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

/// Drive the real `/agent/call_tool` handler and return `(status, body)`.
async fn call_tool(payload: Value) -> (StatusCode, Value) {
    let sandbox = tempfile::tempdir().unwrap();
    let _env = env_lock::lock_env([(
        "BIOROUTER_PATH_ROOT",
        Some(sandbox.path().to_string_lossy().into_owned()),
    )]);
    let state = biorouter_server::state::AppState::new()
        .await
        .expect("app state");
    let app = biorouter_server::routes::agent::routes(state);

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/agent/call_tool")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn body_text(body: &Value) -> String {
    body["content"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|c| c["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// All four operations, in both the named and the whole-store shape. Each one
/// must come back as a refused tool call — never dispatched, and never a bare
/// status code that tells the caller nothing.
#[tokio::test]
#[serial_test::serial]
async fn every_global_memory_operation_is_refused_over_the_route() {
    for (what, name, arguments) in [
        (
            "named read",
            "memory__retrieve_memories",
            json!({"category": "clinical", "is_global": true}),
        ),
        (
            "whole-store read",
            "memory__retrieve_memories",
            json!({"category": "*", "is_global": true}),
        ),
        (
            "write",
            "memory__remember_memory",
            json!({"category": "clinical", "data": "x", "tags": [], "is_global": true}),
        ),
        (
            "named delete",
            "memory__remove_memory_category",
            json!({"category": "clinical", "is_global": true}),
        ),
        (
            "whole-store delete",
            "memory__remove_memory_category",
            json!({"category": "*", "is_global": true}),
        ),
        (
            "entry delete",
            "memory__remove_specific_memory",
            json!({"category": "clinical", "memory_content": "x", "is_global": true}),
        ),
    ] {
        let (status, body) = call_tool(json!({
            "session_id": "route-boundary",
            "name": name,
            "arguments": arguments,
        }))
        .await;

        assert_eq!(status, StatusCode::OK, "{what}: unexpected status");
        assert_eq!(
            body["is_error"],
            json!(true),
            "{what}: the route dispatched a machine-wide memory operation with \
             nothing able to ask the user: {body}"
        );
        let text = body_text(&body);
        assert!(
            text.contains("machine-wide"),
            "{what}: the refusal does not say what was refused: {text}"
        );
        assert!(
            text.contains("conversation"),
            "{what}: the refusal does not point at the route that does work: {text}"
        );
    }
}

/// The guard is scoped: it is the machine-wide store that cannot be reached
/// from here, not the memory tools and not this route. A local memory call and
/// an unrelated tool are left to the handler, which fails them for the ordinary
/// reason (no agent for this session) rather than refusing them as a consent
/// violation.
#[tokio::test]
#[serial_test::serial]
async fn local_and_unrelated_calls_are_not_refused_as_consent_violations() {
    for (what, name, arguments) in [
        (
            "local read",
            "memory__retrieve_memories",
            json!({"category": "*", "is_global": false}),
        ),
        (
            "local write",
            "memory__remember_memory",
            json!({"category": "dev", "data": "x", "tags": [], "is_global": false}),
        ),
        (
            "unrelated tool",
            "developer__shell",
            json!({"command": "echo hi"}),
        ),
    ] {
        let (status, body) = call_tool(json!({
            "session_id": "route-boundary-negative",
            "name": name,
            "arguments": arguments,
        }))
        .await;

        let text = body_text(&body);
        assert!(
            !text.contains("machine-wide"),
            "{what} was refused as a machine-wide memory operation \
             (status {status}): {text}"
        );
    }
}
