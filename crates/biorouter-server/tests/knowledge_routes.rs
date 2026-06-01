/// Integration tests for the /knowledge HTTP routes.
///
/// The router under test takes `Arc<KnowledgeService>` directly, so we don't
/// need to construct a full `AppState`.  Each test gets a fresh tempdir-backed
/// `KnowledgeService` via `build_test_router()`.
use axum::{body::Body, http::Request, Router};
use biorouter_mcp::knowledge::service::KnowledgeService;
use std::sync::Arc;
use tower::ServiceExt;

fn build_test_router() -> (tempfile::TempDir, Router) {
    let dir = tempfile::tempdir().unwrap();
    let svc = Arc::new(KnowledgeService::new(dir.path().to_path_buf()));
    let router = biorouter_server::routes::knowledge::router(svc);
    (dir, router)
}

// ──────────────────────────────────────────────────────────────────────────────
// Task 5: read-only routes
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_bases_empty_returns_empty_array() {
    let (_d, app) = build_test_router();
    let res = app
        .oneshot(
            Request::builder()
                .uri("/bases")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn create_then_get_base() {
    let (_d, app) = build_test_router();
    let create_body =
        serde_json::to_vec(&serde_json::json!({"id": "t", "name": "Test"})).unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bases")
                .header("content-type", "application/json")
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "POST /bases should return 200");

    let res = app
        .oneshot(
            Request::builder()
                .uri("/bases/t")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "GET /bases/t should return 200");
}

#[tokio::test]
async fn get_graph_returns_ok_on_new_kb() {
    let (_d, app) = build_test_router();
    // Create a KB first.
    let create_body =
        serde_json::to_vec(&serde_json::json!({"id": "g", "name": "Graph Test"})).unwrap();
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bases")
                .header("content-type", "application/json")
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();

    let res = app
        .oneshot(
            Request::builder()
                .uri("/bases/g/graph")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "GET /bases/g/graph should return 200");
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // A fresh KB graph has empty nodes/edges.
    assert!(v.get("nodes").is_some(), "graph should have nodes key");
    assert!(v.get("edges").is_some(), "graph should have edges key");
}

// ──────────────────────────────────────────────────────────────────────────────
// Task 6: page CRUD routes
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_pages_empty_on_new_kb() {
    let (_d, app) = build_test_router();
    // Create KB.
    let create_body =
        serde_json::to_vec(&serde_json::json!({"id": "lp", "name": "List Pages"})).unwrap();
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bases")
                .header("content-type", "application/json")
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();

    let res = app
        .oneshot(
            Request::builder()
                .uri("/bases/lp/pages")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        v.as_array().unwrap().len(),
        0,
        "new KB should have no pages"
    );
}

#[tokio::test]
async fn write_then_read_page_roundtrip() {
    let (_d, app) = build_test_router();
    // Create KB.
    let create_body =
        serde_json::to_vec(&serde_json::json!({"id": "wr", "name": "Write Read"})).unwrap();
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bases")
                .header("content-type", "application/json")
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();

    // Write a page.
    let write_body = serde_json::to_vec(&serde_json::json!({
        "content": "# Hello\n\nWorld",
        "commit_message": "add hello page"
    }))
    .unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/bases/wr/pages/knowledge/notes/hello.md")
                .header("content-type", "application/json")
                .body(Body::from(write_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "PUT page should return 200");

    // Read the page back.
    let res = app
        .oneshot(
            Request::builder()
                .uri("/bases/wr/pages/knowledge/notes/hello.md")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "GET page should return 200");
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        v["content"].as_str().unwrap().contains("Hello"),
        "page content should contain written text"
    );
}

#[tokio::test]
async fn read_page_on_missing_path_returns_404() {
    let (_d, app) = build_test_router();
    // Create KB.
    let create_body =
        serde_json::to_vec(&serde_json::json!({"id": "miss", "name": "Missing"})).unwrap();
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bases")
                .header("content-type", "application/json")
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();

    let res = app
        .oneshot(
            Request::builder()
                .uri("/bases/miss/pages/knowledge/notes/nonexistent.md")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        404,
        "reading a missing page should return 404"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Task 7: history + preview + restore
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn history_write_restore_roundtrip() {
    let (_d, app) = build_test_router();

    // 1. Create base.
    let create_body =
        serde_json::to_vec(&serde_json::json!({"id": "hist", "name": "History"})).unwrap();
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bases")
                .header("content-type", "application/json")
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();

    // 2. list_history after create → 1 entry (the initial commit).
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/bases/hist/history")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let history: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let history_arr = history.as_array().unwrap();
    assert!(
        !history_arr.is_empty(),
        "should have at least the initial commit"
    );
    // Capture the first (oldest) commit SHA.
    let first_sha = history_arr
        .last()
        .unwrap()
        .get("commit_sha")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // 3. Write a page.
    let write_body = serde_json::to_vec(&serde_json::json!({
        "content": "# Temp Page",
        "commit_message": "add temp page"
    }))
    .unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/bases/hist/pages/knowledge/notes/temp.md")
                .header("content-type", "application/json")
                .body(Body::from(write_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "write page should succeed");

    // 4. list_history should now have 2+ entries.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/bases/hist/history")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let history2: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        history2.as_array().unwrap().len() >= 2,
        "should have at least 2 commits after write"
    );

    // 5. restore_state to the first commit (before the page write).
    let restore_body =
        serde_json::to_vec(&serde_json::json!({"commit_sha": first_sha})).unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bases/hist/restore")
                .header("content-type", "application/json")
                .body(Body::from(restore_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "restore should succeed");
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let restore_resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        restore_resp.get("new_commit_sha").is_some(),
        "restore should return new_commit_sha"
    );

    // 6. list_history again → 3+ entries (initial + write + restore).
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/bases/hist/history")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let history3: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        history3.as_array().unwrap().len() >= 3,
        "should have 3+ entries after restore"
    );

    // 7. The page should be gone after restore to initial commit.
    let res = app
        .oneshot(
            Request::builder()
                .uri("/bases/hist/pages/knowledge/notes/temp.md")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        404,
        "page should not exist after restoring to initial commit"
    );
}
