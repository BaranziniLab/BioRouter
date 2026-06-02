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

/// Build a router and also return the underlying KB root directory so tests
/// can seed files directly on disk (needed for routes that read from `raw/`
/// where there is no write API).
fn build_test_router_with_root() -> (tempfile::TempDir, std::path::PathBuf, Router) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let svc = Arc::new(KnowledgeService::new(root.clone()));
    let router = biorouter_server::routes::knowledge::router(svc);
    (dir, root, router)
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
    let create_body = serde_json::to_vec(&serde_json::json!({"id": "t", "name": "Test"})).unwrap();
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
async fn update_base_metadata_roundtrip() {
    let (_d, app) = build_test_router();
    let create_body =
        serde_json::to_vec(&serde_json::json!({"id": "rename", "name": "Original"})).unwrap();
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

    let update_body = serde_json::to_vec(&serde_json::json!({
        "name": "Renamed Knowledge Base",
        "color": "#123456"
    }))
    .unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/bases/rename")
                .header("content-type", "application/json")
                .body(Body::from(update_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "PUT /bases/rename should return 200");

    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(manifest["id"], "renamed-knowledge-base");
    assert_eq!(manifest["name"], "Renamed Knowledge Base");
    assert_eq!(manifest["color"], "#123456");

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/bases/renamed-knowledge-base")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        200,
        "GET /bases/renamed-knowledge-base should return 200"
    );
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(manifest["id"], "renamed-knowledge-base");
    assert_eq!(manifest["name"], "Renamed Knowledge Base");
    assert_eq!(manifest["color"], "#123456");

    let res = app
        .oneshot(
            Request::builder()
                .uri("/bases/rename")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        404,
        "GET /bases/rename should return 404 after rename"
    );
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
    let restore_body = serde_json::to_vec(&serde_json::json!({"commit_sha": first_sha})).unwrap();
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

// ──────────────────────────────────────────────────────────────────────────────
// Task 8: POST /bases/:id/raw
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn add_raw_source_text() {
    let (_d, app) = build_test_router();

    // Create a KB.
    let create_body =
        serde_json::to_vec(&serde_json::json!({"id": "raw", "name": "Raw Test"})).unwrap();
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

    // POST a text source.
    let body = serde_json::to_vec(&serde_json::json!({
        "text": "hello world lab note",
        "title": "Note"
    }))
    .unwrap();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bases/raw/raw")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "POST /bases/raw/raw should return 200");
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        v.get("source_id").is_some(),
        "response should have source_id"
    );
    assert!(
        v.get("source_md_path").is_some(),
        "response should have source_md_path"
    );
}

#[tokio::test]
async fn add_raw_source_html_multipart_uses_part_mime() {
    let (_d, app) = build_test_router();

    let create_body =
        serde_json::to_vec(&serde_json::json!({"id": "rawhtml", "name": "Raw Html"})).unwrap();
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
        .clone()
        .oneshot(multipart_file_request(
            "/bases/rawhtml/raw",
            "upload",
            Some("text/html; charset=utf-8"),
            b"<html><body><h1>Hello</h1><p>World</p></body></html>".to_vec(),
            &[],
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let path = v["source_md_path"].as_str().unwrap();

    let res = app
        .oneshot(
            Request::builder()
                .uri(format!("/bases/rawhtml/page?path={path}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let page: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(page["content"].as_str().unwrap().contains("# Hello"));
}

#[tokio::test]
async fn add_raw_source_rejects_empty_body() {
    let (_d, app) = build_test_router();

    // Create a KB.
    let create_body =
        serde_json::to_vec(&serde_json::json!({"id": "rj", "name": "Reject"})).unwrap();
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

    // POST an empty JSON object — no url, no text, no file.
    let body = serde_json::to_vec(&serde_json::json!({})).unwrap();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bases/rj/raw")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 400, "empty body should return 400");
}

// ──────────────────────────────────────────────────────────────────────────────
// Task 10: GET /bases/:id/export + POST /bases/import
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn export_then_import_roundtrip() {
    let (_d, app) = build_test_router();

    // 1. Create a KB.
    let create_body =
        serde_json::to_vec(&serde_json::json!({"id": "ex", "name": "Export"})).unwrap();
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

    // 2. Add a small text source so the archive has content.
    let source_body = serde_json::to_vec(&serde_json::json!({
        "text": "content for export",
        "title": "Test"
    }))
    .unwrap();
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bases/ex/raw")
                .header("content-type", "application/json")
                .body(Body::from(source_body))
                .unwrap(),
        )
        .await
        .unwrap();

    // 3. Export the KB.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/bases/ex/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "GET /bases/ex/export should return 200");
    assert_eq!(
        res.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("application/octet-stream"),
        "export should return octet-stream"
    );
    let brkb_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(
        brkb_bytes.len() > 100,
        "exported archive should have content"
    );

    // 4. Import via multipart — build a minimal multipart body by hand.
    // Boundary and body structure per RFC 2046.
    let boundary = "TESTBOUNDARY";
    let mut multipart_body: Vec<u8> = Vec::new();
    multipart_body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"ex.brkb\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    multipart_body.extend_from_slice(&brkb_bytes);
    multipart_body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bases/import")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(multipart_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "POST /bases/import should return 200");
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let new_id = v.get("id").and_then(|v| v.as_str()).unwrap();
    // The original id was "ex" and it already exists, so the import should assign "ex-2".
    assert_eq!(
        new_id, "ex-2",
        "import into existing root should suffix with -2"
    );

    // 5. The imported KB should show up in list_bases (we should now have 2 bases).
    let res = app
        .oneshot(
            Request::builder()
                .uri("/bases")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let bases: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let count = bases.as_array().unwrap().len();
    assert_eq!(
        count, 2,
        "should have 2 knowledge bases after import (original + imported)"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Task 11: reclassify + override_credibility routes
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn reclassify_route_returns_credibility() {
    let (_d, app) = build_test_router();

    // Create KB.
    let create_body =
        serde_json::to_vec(&serde_json::json!({"id": "rc", "name": "Reclassify"})).unwrap();
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

    // Add a text source.
    let source_body = serde_json::to_vec(&serde_json::json!({
        "text": "personal research note",
        "title": "Note"
    }))
    .unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bases/rc/raw")
                .header("content-type", "application/json")
                .body(Body::from(source_body))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let source: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let source_id = source.get("source_id").and_then(|v| v.as_str()).unwrap();

    // Reclassify.
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/bases/rc/sources/{source_id}/reclassify"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "reclassify should return 200");
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        v.get("credibility").is_some(),
        "response should have credibility"
    );
    assert!(
        v["credibility"].get("tier").is_some(),
        "credibility should have tier"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Task 9: SSE macro routes — 400 on invalid model
// ──────────────────────────────────────────────────────────────────────────────

/// Helper: create a KB named `id` in `app`, assert 200.
async fn create_kb(app: Router, id: &str, name: &str) {
    let create_body = serde_json::to_vec(&serde_json::json!({"id": id, "name": name})).unwrap();
    let res = app
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
    assert_eq!(res.status(), 200, "helper create_kb should return 200");
}

fn multipart_file_request(
    uri: &str,
    filename: &str,
    file_content_type: Option<&str>,
    bytes: Vec<u8>,
    fields: &[(&str, &str)],
) -> Request<Body> {
    let boundary = "XBOUNDARY";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    if let Some(content_type) = file_content_type {
        body.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
    }
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(&bytes);
    for (name, value) in fields {
        body.extend_from_slice(format!("\r\n--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
    }
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    Request::builder()
        .method("POST")
        .uri(uri)
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

fn ingest_multipart_request(
    uri: &str,
    filename: &str,
    file_content_type: Option<&str>,
    bytes: Vec<u8>,
    provider: &str,
    model: &str,
) -> Request<Body> {
    multipart_file_request(
        uri,
        filename,
        file_content_type,
        bytes,
        &[("provider", provider), ("model", model)],
    )
}

#[tokio::test]
async fn ingest_rejects_invalid_model_with_400() {
    let (_d, app) = build_test_router();
    create_kb(app.clone(), "ing", "Ingest Test").await;

    let body = serde_json::to_vec(&serde_json::json!({
        "source": {"text": "hello", "title": "x"},
        "model": {"provider": "nonexistent_provider_xyz", "model": "x"}
    }))
    .unwrap();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bases/ing/ingest")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        400,
        "ingest with unknown provider should return 400"
    );
}

#[tokio::test]
async fn expand_path_returns_stageable_children_for_directory() {
    let (_d, app) = build_test_router();
    let temp = tempfile::tempdir().unwrap();
    let bundle_dir = temp.path().join("bundle");
    std::fs::create_dir_all(bundle_dir.join("docs")).unwrap();
    std::fs::write(bundle_dir.join("docs/readme.md"), "# Archive file").unwrap();

    let body = serde_json::to_vec(&serde_json::json!({
        "path": bundle_dir.to_string_lossy()
    }))
    .unwrap();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/expand-path")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["files"].as_array().unwrap().len(), 1);
    assert!(v["files"][0]["relative_path"]
        .as_str()
        .unwrap()
        .contains("bundle/docs/readme.md"));
}

#[tokio::test]
async fn expand_path_cleans_archive_wrapper_and_metadata_noise() {
    let (_d, app) = build_test_router();
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("bundle.zip");
    let file = std::fs::File::create(&archive_path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    use std::io::Write;
    zip.start_file("bundle/docs/readme.md", options).unwrap();
    zip.write_all(b"# Archive child").unwrap();
    zip.start_file("__MACOSX/bundle/docs/._readme.md", options)
        .unwrap();
    zip.write_all(b"metadata").unwrap();
    zip.finish().unwrap();

    let body = serde_json::to_vec(&serde_json::json!({
        "path": archive_path.to_string_lossy()
    }))
    .unwrap();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/expand-path")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["files"].as_array().unwrap().len(), 1);
    assert_eq!(v["files"][0]["relative_path"], "bundle/docs/readme.md");
    assert!(v["warnings"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn ingest_rejects_brkb_uploads_in_dropzone_route() {
    let (_d, app) = build_test_router();
    create_kb(app.clone(), "ing", "Ingest Test").await;

    let res = app
        .oneshot(ingest_multipart_request(
            "/bases/ing/ingest",
            "archive.brkb",
            None,
            b"not-a-real-archive".to_vec(),
            "test-provider",
            "test-model",
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn ingest_rejects_oversized_csv_uploads_before_model_check() {
    let (_d, app) = build_test_router();
    create_kb(app.clone(), "ing", "Ingest Test").await;

    let res = app
        .oneshot(ingest_multipart_request(
            "/bases/ing/ingest",
            "huge.csv",
            None,
            vec![b'a'; 8 * 1024 * 1024 + 1],
            "test-provider",
            "test-model",
        ))
        .await
        .unwrap();

    assert!(
        res.status() == 400 || res.status() == 413,
        "oversized uploads should be rejected before digestion starts, got {}",
        res.status()
    );
}

#[tokio::test]
async fn ingest_accepts_html_upload_before_model_validation() {
    let (_d, app) = build_test_router();
    create_kb(app.clone(), "ing", "Ingest Test").await;

    let res = app
        .oneshot(ingest_multipart_request(
            "/bases/ing/ingest",
            "upload",
            Some("text/html; charset=utf-8"),
            b"<html><body><h1>Hello</h1></body></html>".to_vec(),
            "nonexistent_provider_xyz",
            "x",
        ))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        400,
        "multipart HTML should parse successfully and then fail on invalid model"
    );
}

#[tokio::test]
async fn query_rejects_invalid_model_with_400() {
    let (_d, app) = build_test_router();
    create_kb(app.clone(), "qry", "Query Test").await;

    let body = serde_json::to_vec(&serde_json::json!({
        "question": "What is HRV?",
        "model": {"provider": "nonexistent_provider_xyz", "model": "x"}
    }))
    .unwrap();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bases/qry/query")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        400,
        "query with unknown provider should return 400"
    );
}

#[tokio::test]
async fn lint_rejects_invalid_model_with_400_when_autofix() {
    let (_d, app) = build_test_router();
    create_kb(app.clone(), "lnt", "Lint Test").await;

    // autofix=true requires a real completer → 400 on bad provider.
    let body = serde_json::to_vec(&serde_json::json!({
        "model": {"provider": "nonexistent_provider_xyz", "model": "x"},
        "autofix": true
    }))
    .unwrap();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bases/lnt/lint")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        400,
        "lint with unknown provider + autofix=true should return 400"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// check_model route
// ──────────────────────────────────────────────────────────────────────────────

// ──────────────────────────────────────────────────────────────────────────────
// Plan 5 Task 1: GET /bases/:id/page?path=... — markdown body for NodePreview
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn read_page_returns_markdown_body() {
    let (_d, root, app) = build_test_router_with_root();

    // Create the KB.
    let create_body =
        serde_json::to_vec(&serde_json::json!({"id": "rp", "name": "Read Page"})).unwrap();
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
    assert_eq!(res.status(), 200);

    // Seed a knowledge/ page directly on disk.
    let knowledge_dir = root.join("rp").join("knowledge").join("notes");
    std::fs::create_dir_all(&knowledge_dir).unwrap();
    std::fs::write(
        knowledge_dir.join("hello.md"),
        "---\ntitle: Hello\nkind: note\n---\n\nbody text\n",
    )
    .unwrap();

    // Happy path: GET /bases/rp/page?path=knowledge/notes/hello.md
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/bases/rp/page?path=knowledge/notes/hello.md")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "GET /bases/rp/page should return 200");
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        v["content"].as_str().unwrap().contains("body text"),
        "response content should contain page body, got: {v}"
    );

    // Missing page → 404.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/bases/rp/page?path=knowledge/notes/nope.md")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 404, "missing page should return 404");

    // Path traversal → 400. `../../etc/passwd` is rejected by the
    // `starts_with("knowledge/")` / `starts_with("raw/")` allowlist *before*
    // the `..` check ever fires.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/bases/rp/page?path=../../etc/passwd")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        400,
        "path traversal should return 400 not 500"
    );

    // Real traversal: passes the prefix allowlist (`starts_with("knowledge/")`)
    // but contains `..` — must be rejected by the dedicated traversal check.
    // This exercises a different code path than the test above.
    let res = app
        .oneshot(
            Request::builder()
                .uri("/bases/rp/page?path=knowledge/../../etc/passwd")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        400,
        "real path-traversal attempt should return 400 not 500"
    );
}

#[tokio::test]
async fn read_page_returns_raw_source_md() {
    let (_d, root, app) = build_test_router_with_root();

    // Create the KB.
    let create_body =
        serde_json::to_vec(&serde_json::json!({"id": "rs", "name": "Raw Source"})).unwrap();
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
    assert_eq!(res.status(), 200);

    // Seed a raw/<src-id>/source.md file directly on disk so we don't have to
    // round-trip through the converter.
    let raw_dir = root.join("rs").join("raw").join("test");
    std::fs::create_dir_all(&raw_dir).unwrap();
    std::fs::write(raw_dir.join("source.md"), "# raw\n\nraw source body\n").unwrap();

    // Happy path: GET /bases/rs/page?path=raw/test/source.md
    let res = app
        .oneshot(
            Request::builder()
                .uri("/bases/rs/page?path=raw/test/source.md")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        200,
        "GET /bases/rs/page?path=raw/.../source.md should return 200"
    );
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        v["content"].as_str().unwrap().contains("raw source body"),
        "response content should contain raw source body, got: {v}"
    );
}

#[tokio::test]
async fn read_page_rejects_invalid_kb_id_with_400() {
    // Regression test for the dead-branch bug: the handler previously checked
    // for "invalid kb id" in the error string, but `validate_kb_id` emits
    // "kb-id may only contain a-z, 0-9, and '-'" (etc.). The mismatch meant an
    // invalid kb-id slipped through to 500. With the typed-error refactor this
    // must now route to 400.
    let (_d, app) = build_test_router();

    // "INVALID--KB" violates both the lowercase rule and the `--` rule. We do
    // not need to create the KB; validation fires before any filesystem touch.
    let res = app
        .oneshot(
            Request::builder()
                .uri("/bases/INVALID--KB/page?path=knowledge/x.md")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        400,
        "invalid kb-id must return 400, not 500 (regression test)"
    );
}

#[tokio::test]
async fn check_model_returns_502_for_unknown_provider() {
    let (_d, app) = build_test_router();

    let body = serde_json::to_vec(&serde_json::json!({
        "model": {"provider": "nonexistent_provider_xyz", "model": "x"}
    }))
    .unwrap();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/check-model")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    // The provider build fails → 502 Bad Gateway with ok: false
    assert_eq!(
        res.status(),
        502,
        "check_model with unknown provider should return 502"
    );
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["ok"], false, "ok should be false");
    assert!(
        v.get("error").is_some(),
        "response should have an error field"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Plan 6 Task 2: GET + POST /knowledge/active — cross-session active-KB sync
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn active_kb_roundtrip() {
    let (_d, app) = build_test_router();

    // Empty initially.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/active")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "GET /active on fresh root should be 200");
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        v["active_kb"].is_null(),
        "active_kb should be null on a fresh root"
    );

    // Create a KB to point at.
    let create_body = serde_json::to_vec(&serde_json::json!({"id": "act", "name": "Act"})).unwrap();
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

    // Set it.
    let set_body = serde_json::to_vec(&serde_json::json!({"kb_id": "act"})).unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/active")
                .header("content-type", "application/json")
                .body(Body::from(set_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        200,
        "POST /active with valid id should be 200"
    );

    // Read it back.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/active")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let after: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        after["active_kb"].as_str().unwrap(),
        "act",
        "active_kb should round-trip the set value"
    );

    // Clear it.
    let clear_body = serde_json::to_vec(&serde_json::json!({"kb_id": null})).unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/active")
                .header("content-type", "application/json")
                .body(Body::from(clear_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "POST /active with null should clear");

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/active")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let cleared: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        cleared["active_kb"].is_null(),
        "active_kb should be null after clear"
    );

    // Invalid kb id returns 400.
    let bad_body = serde_json::to_vec(&serde_json::json!({"kb_id": "INVALID--KB"})).unwrap();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/active")
                .header("content-type", "application/json")
                .body(Body::from(bad_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        400,
        "POST /active with invalid kb id should return 400"
    );
}

#[tokio::test]
async fn active_kb_can_be_scoped_per_session() {
    let (_d, app) = build_test_router();

    for (id, name) in [("act", "Act"), ("session-kb", "Session KB")] {
        let create_body = serde_json::to_vec(&serde_json::json!({"id": id, "name": name})).unwrap();
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
        assert_eq!(res.status(), 200);
    }

    let global_body = serde_json::to_vec(&serde_json::json!({"kb_id": "act"})).unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/active")
                .header("content-type", "application/json")
                .body(Body::from(global_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/active?session_id=session-a")
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
    assert_eq!(v["active_kb"].as_str(), Some("act"));

    let session_body = serde_json::to_vec(&serde_json::json!({
        "kb_id": "session-kb",
        "session_id": "session-a"
    }))
    .unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/active")
                .header("content-type", "application/json")
                .body(Body::from(session_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/active?session_id=session-a")
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
    assert_eq!(v["active_kb"].as_str(), Some("session-kb"));

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/active")
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
    assert_eq!(v["active_kb"].as_str(), Some("act"));
}
