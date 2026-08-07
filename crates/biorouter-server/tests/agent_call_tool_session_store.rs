//! `POST /agent/call_tool` against the session database (issue #56).
//!
//! The route hands a tool call straight to `ExtensionManager::dispatch_tool_call`
//! — no agent loop, so no `ToolInspector`, so no `SessionStoreInspector`. It is
//! one of the two production doors that reach a filesystem tool with nothing
//! inspecting the arguments, and the store it reaches is not one conversation's
//! data: it is every conversation any project on this machine ever ran, private
//! ones included.
//!
//! ⚠ **These tests assert the DECISION, not a prevented read**, and that is a
//! property of the route rather than a weakness of the tests: the refusal is
//! taken before the session's agent is resolved, on purpose — a decision that
//! needed one could be skipped by arriving without one — so a request that gets
//! past it stops at "no agent for this session" here rather than opening the
//! file. What fails without the guard is that the answer is no longer a refusal
//! naming the store. This is the same shape as
//! `agent_call_tool_global_memory.rs`, for the same reason.

// Redirects this binary's Biorouter data/config/state dirs at a throwaway root
// before `main`, so nothing here can name the developer's real `sessions.db`.
#[path = "../src/test_sandbox.rs"]
mod test_sandbox;

use axum::{body::Body, http::Request, http::StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

/// Drive the real `/agent/call_tool` handler with `BIOROUTER_PATH_ROOT` pinned
/// to `root`, so the store the guard resolves is under a directory this test
/// owns.
///
/// The env guard is held across the request: `Paths` reads the variable on every
/// call, so releasing it before `oneshot` would move the barrier back onto
/// whatever this process's data dir is and quietly test something else.
async fn call_tool(root: &std::path::Path, payload: Value) -> (StatusCode, Value) {
    let _env = env_lock::lock_env([(
        "BIOROUTER_PATH_ROOT",
        Some(root.to_string_lossy().into_owned()),
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

/// The store as `Paths::data_dir()` + `sessions/` + `sessions.db` resolves it
/// under a pinned `BIOROUTER_PATH_ROOT`.
fn store_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.join("data").join("sessions")
}

/// Every shape that reaches the transcript store through this route: the
/// database, its write-ahead log (which holds the *newest* turns), the directory
/// itself, a path argument rather than a command line, and a write.
///
/// Each must come back as a refused tool call — never dispatched, and never a
/// bare status code that tells the caller nothing.
#[tokio::test]
#[serial_test::serial]
async fn every_route_to_the_session_store_is_refused_over_the_route() {
    let root = tempfile::tempdir().unwrap();
    let dir = store_dir(root.path());
    let db = dir.join("sessions.db");

    for (what, name, arguments) in [
        (
            "read the database",
            "developer__shell",
            json!({ "command": format!("cat {}", db.display()) }),
        ),
        (
            "query it with sqlite3",
            "developer__shell",
            json!({ "command": format!("sqlite3 {} \"select content from messages\"", db.display()) }),
        ),
        (
            "read the write-ahead log, which holds the newest turns",
            "developer__shell",
            json!({ "command": format!("cat {}-wal", db.display()) }),
        ),
        (
            "copy the whole store directory out",
            "developer__shell",
            json!({ "command": format!("tar czf /tmp/x.tgz {}", dir.display()) }),
        ),
        (
            "a path argument rather than a command line",
            "developer__text_editor",
            json!({ "command": "view", "path": db.display().to_string() }),
        ),
        (
            "overwrite it",
            "developer__shell",
            json!({ "command": format!("echo wiped > {}", db.display()) }),
        ),
    ] {
        let (status, body) = call_tool(
            root.path(),
            json!({
                "session_id": "session-store-route-boundary",
                "name": name,
                "arguments": arguments,
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "{what}: unexpected status");
        assert_eq!(
            body["is_error"],
            json!(true),
            "{what}: the route dispatched a tool at the transcript store with no inspector \
             anywhere: {body}"
        );
        let text = body_text(&body);
        assert!(
            text.contains("session database"),
            "{what}: the refusal does not say what was refused: {text}"
        );
        assert!(
            text.contains("chatrecall"),
            "{what}: the refusal does not name the route that still works: {text}"
        );
    }
}

/// The guard is scoped to the store, not to this route and not to the
/// filesystem. A gate that fired on ordinary work is a gate someone switches
/// off — and these are the near misses that would do it: a directory whose name
/// merely starts with the store's, a project's own file sharing the basename,
/// and a command that has nothing to do with any of it.
///
/// Each is left to the handler, which fails it for the ordinary reason (no agent
/// for this session) rather than refusing it as a store read.
#[tokio::test]
#[serial_test::serial]
async fn ordinary_calls_and_near_misses_are_not_refused_as_store_reads() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");

    for (what, name, arguments) in [
        (
            "unrelated command",
            "developer__shell",
            json!({ "command": "echo hi" }),
        ),
        (
            "the data directory itself",
            "developer__shell",
            json!({ "command": format!("ls {}", data.display()) }),
        ),
        (
            "a directory whose name merely starts with the store's",
            "developer__shell",
            json!({ "command": format!("cat {}/sessions-archive/sessions.db", data.display()) }),
        ),
        (
            "a project's own file sharing the basename",
            "developer__shell",
            json!({ "command": "sqlite3 ./data/sessions.db .schema" }),
        ),
        (
            "documenting the store, rather than reading it",
            "developer__text_editor",
            json!({
                "command": "write",
                "path": "docs/privacy/session-store.md",
                "file_text": format!(
                    "Transcripts live in {}/sessions/sessions.db.\n", data.display()
                ),
            }),
        ),
    ] {
        let (status, body) = call_tool(
            root.path(),
            json!({
                "session_id": "session-store-route-negative",
                "name": name,
                "arguments": arguments,
            }),
        )
        .await;

        let text = body_text(&body);
        assert!(
            !text.contains("session database"),
            "{what} was refused as a session-store read (status {status}): {text}"
        );
    }
}
