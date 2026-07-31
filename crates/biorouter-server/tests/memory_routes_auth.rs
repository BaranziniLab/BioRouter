//! The `/memory` routes as the daemon actually serves them (issue #63 review).
//!
//! `memory_routes.rs` drives a router built by
//! `memory::router_with_global_store`, which is the right seam for behaviour —
//! it binds a throwaway store so a test never deletes the developer's real
//! memories — but it proves nothing about the two things a caller depends on in
//! production:
//!
//! * that the routes are **registered** at all. `routes::configure` is a list of
//!   `.merge(...)` calls; dropping one line takes a whole feature off the daemon
//!   while every route-level test keeps passing.
//! * that they are **authenticated**. These routes list the machine-wide memory
//!   store and irreversibly delete from it. The daemon's `check_token`
//!   middleware is the only thing between them and anything that can reach the
//!   loopback port, and it is applied in `commands/agent.rs`, outside
//!   `configure` — so nothing in the crate's tests observed it.
//!
//! This binary composes both the way the daemon does and asserts on the result.
//! The store is sandboxed with `BIOROUTER_PATH_ROOT`, which
//! `biorouter_mcp::global_memory_dir` honours, so the production wiring reads a
//! temporary directory rather than `~/.config/biorouter/memory`.

use axum::{body::Body, http::Request, http::StatusCode, middleware, Router};
use biorouter_server::auth::check_token;
use tower::ServiceExt;

const SECRET: &str = "the-daemons-secret-key";

/// The daemon's own composition: every route module merged by
/// `routes::configure`, wrapped in the same secret-key middleware
/// `commands/agent.rs` wraps it in.
async fn daemon(root: &std::path::Path) -> Router {
    let _env = env_lock::lock_env([(
        "BIOROUTER_PATH_ROOT",
        Some(root.to_string_lossy().into_owned()),
    )]);
    let state = biorouter_server::state::AppState::new()
        .await
        .expect("app state");
    biorouter_server::routes::configure(state, SECRET.to_string()).layer(
        middleware::from_fn_with_state(SECRET.to_string(), check_token),
    )
}

/// Where `global_memory_dir()` lands under a sandbox root.
fn store(root: &std::path::Path) -> std::path::PathBuf {
    root.join("config").join("memory")
}

async fn send(app: Router, request: Request<Body>) -> (StatusCode, String) {
    let res = app.oneshot(request).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn get(uri: &str, secret: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().uri(uri);
    if let Some(secret) = secret {
        builder = builder.header("X-Secret-Key", secret);
    }
    builder.body(Body::empty()).unwrap()
}

fn post(uri: &str, secret: Option<&str>, payload: serde_json::Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(secret) = secret {
        builder = builder.header("X-Secret-Key", secret);
    }
    builder.body(Body::from(payload.to_string())).unwrap()
}

/// The routes exist on the daemon, and the secret key reaches them.
#[tokio::test]
#[serial_test::serial]
async fn the_memory_routes_are_registered_on_the_daemon() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(store(root.path())).unwrap();
    std::fs::write(store(root.path()).join("clinical.txt"), "a note\n\n").unwrap();

    let (status, body) = send(
        daemon(root.path()).await,
        get("/memory/inventory", Some(SECRET)),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the inventory route is not reachable through routes::configure: {body}"
    );
    assert!(
        body.contains("clinical"),
        "the daemon's route resolved a different global store than \
         BIOROUTER_PATH_ROOT names: {body}"
    );
}

/// Without the secret, nothing about the store may come back — not its contents,
/// not its path, not whether it exists. And a delete must not happen.
#[tokio::test]
#[serial_test::serial]
async fn the_memory_routes_refuse_a_request_with_no_secret() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(store(root.path())).unwrap();
    std::fs::write(
        store(root.path()).join("clinical.txt"),
        "PATIENT-SECRET-8811\n\n",
    )
    .unwrap();

    let (status, body) = send(daemon(root.path()).await, get("/memory/inventory", None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        !body.contains("PATIENT-SECRET-8811"),
        "an unauthenticated caller was shown the machine-wide store: {body}"
    );

    let (status, _) = send(
        daemon(root.path()).await,
        post(
            "/memory/delete_category",
            None,
            serde_json::json!({"scope": "global", "category": "clinical", "revision": "any"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        store(root.path()).join("clinical.txt").exists(),
        "an unauthenticated caller deleted a global memory category"
    );
}

/// A wrong key is a wrong key, not a near miss.
#[tokio::test]
#[serial_test::serial]
async fn the_memory_routes_refuse_a_request_with_the_wrong_secret() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(store(root.path())).unwrap();

    let (status, _) = send(
        daemon(root.path()).await,
        get("/memory/inventory", Some("the-daemons-secret-keY")),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
