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

async fn post_active(app: &Router, body: serde_json::Value) -> (u16, serde_json::Value) {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/active")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status().as_u16();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

async fn get_active(app: &Router, session_id: Option<&str>) -> serde_json::Value {
    let uri = match session_id {
        Some(sid) => format!("/active?session_id={sid}"),
        None => "/active".to_string(),
    };
    let res = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
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

#[tokio::test]
async fn get_location_returns_kb_path() {
    let (_d, root, app) = build_test_router_with_root();
    // Create a KB so the directory exists.
    let create_body = serde_json::to_vec(&serde_json::json!({"id": "loc", "name": "Loc"})).unwrap();
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
                .uri("/bases/loc/location")
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
    let path = v.get("path").and_then(|p| p.as_str()).unwrap();
    assert!(
        path.contains("loc"),
        "path should point at the KB dir: {path}"
    );
    assert!(
        path.starts_with(&root.to_string_lossy().into_owned()),
        "path should be under the test root"
    );
}

#[tokio::test]
async fn get_location_404_for_unknown_kb() {
    let (_d, app) = build_test_router();
    let res = app
        .oneshot(
            Request::builder()
                .uri("/bases/nope/location")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
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
async fn set_default_model_persists_on_manifest() {
    let (_d, app) = build_test_router();
    create_kb(app.clone(), "models", "Models").await;

    let body = serde_json::to_vec(&serde_json::json!({
        "model": {
            "provider": "versa_azure",
            "model": "gpt-5.5-2026-04-24"
        }
    }))
    .unwrap();

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/bases/models/default-model")
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
    let manifest: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(manifest["default_model"]["provider"], "versa_azure");
    assert_eq!(manifest["default_model"]["model"], "gpt-5.5-2026-04-24");

    let res = app
        .oneshot(
            Request::builder()
                .uri("/bases/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(manifest["default_model"]["provider"], "versa_azure");
    assert_eq!(manifest["default_model"]["model"], "gpt-5.5-2026-04-24");
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
async fn add_raw_source_accepts_supported_file_formats() {
    let (_d, app) = build_test_router();

    struct Case<'a> {
        slug: &'a str,
        filename: &'a str,
        mime: Option<&'a str>,
        bytes: Vec<u8>,
        expected: Option<&'a str>,
    }

    let cases = vec![
        Case {
            slug: "html",
            filename: "article.html",
            mime: Some("text/html"),
            bytes: include_bytes!(
                "../../biorouter-mcp/src/knowledge/convert/fixtures/article.html"
            )
            .to_vec(),
            expected: Some("# Example article"),
        },
        Case {
            slug: "markdown",
            filename: "note.md",
            mime: Some("text/markdown"),
            bytes: b"# Markdown source\n\nDigest this note.".to_vec(),
            expected: Some("# Markdown source"),
        },
        Case {
            slug: "text",
            filename: "note.txt",
            mime: Some("text/plain"),
            bytes: b"Plain text note for digestion.".to_vec(),
            expected: Some("Plain text note for digestion."),
        },
        Case {
            slug: "csv",
            filename: "table.csv",
            mime: Some("text/csv"),
            bytes: b"name,score\nAlice,9\nBob,7\n".to_vec(),
            expected: Some("| name | score |"),
        },
        Case {
            slug: "docx",
            filename: "sample.docx",
            mime: Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
            bytes: include_bytes!(
                "../../biorouter-mcp/src/computercontroller/tests/data/sample.docx"
            )
            .to_vec(),
            expected: None,
        },
        Case {
            slug: "pdf",
            filename: "sample.pdf",
            mime: Some("application/pdf"),
            bytes: include_bytes!("../../biorouter-mcp/src/computercontroller/tests/data/test.pdf")
                .to_vec(),
            expected: None,
        },
    ];

    for (index, case) in cases.into_iter().enumerate() {
        let kb_id = format!("fmt{index}");
        create_kb(app.clone(), &kb_id, case.slug).await;

        let res = app
            .clone()
            .oneshot(multipart_file_request(
                &format!("/bases/{kb_id}/raw"),
                case.filename,
                case.mime,
                case.bytes,
                &[],
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), 200, "{} upload should return 200", case.slug);

        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let path = v["source_md_path"].as_str().unwrap();

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/bases/{kb_id}/page?path={path}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), 200, "{} page should be readable", case.slug);

        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let page: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let content = page["content"].as_str().unwrap();
        if let Some(expected) = case.expected {
            assert!(
                content.contains(expected),
                "{} converted content should contain {expected:?}, got {content:?}",
                case.slug
            );
        } else {
            assert!(
                !content.trim().is_empty(),
                "{} converted content should not be empty",
                case.slug
            );
        }
    }
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
    assert_eq!(
        v["hidden_kbs"].as_array().map(|items| items.len()),
        Some(0),
        "hidden_kbs should be empty on a fresh root"
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
    let set_body = serde_json::to_vec(&serde_json::json!({
        "kb_id": "act",
        "hidden_kbs": ["hidden-a"]
    }))
    .unwrap();
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
    assert_eq!(
        after["hidden_kbs"],
        serde_json::json!(["hidden-a"]),
        "hidden_kbs should round-trip the set value"
    );

    // Clearing is now an explicit flag. A body that simply does not mention
    // the primary leaves it alone, so a hidden-only edit can never nuke it —
    // the same composability rule the app-grant fix relies on.
    let clear_body = serde_json::to_vec(&serde_json::json!({"clear_primary": true})).unwrap();
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
    assert_eq!(
        cleared["hidden_kbs"],
        serde_json::json!(["hidden-a"]),
        "clearing active_kb should not reset hidden_kbs"
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
async fn primary_kb_can_be_scoped_per_session() {
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

    // Machine-wide: both bases in play, "act" is the primary.
    let global = post_active(&app, serde_json::json!({"primary_kb": "act"})).await;
    assert_eq!(global.0, 200);
    assert_eq!(global.1["primary_kb"].as_str(), Some("act"));
    assert_eq!(
        global.1["kb_ids"],
        serde_json::json!(["act", "session-kb"]),
        "the response carries the session's whole set, not just the pointer"
    );

    // session-a narrows to one base and points at it.
    let scoped = post_active(
        &app,
        serde_json::json!({
            "primary_kb": "session-kb",
            "session_id": "session-a",
            "hidden_kbs": ["act"],
        }),
    )
    .await;
    assert_eq!(scoped.0, 200);
    assert_eq!(scoped.1["primary_kb"].as_str(), Some("session-kb"));
    assert_eq!(scoped.1["kb_ids"], serde_json::json!(["session-kb"]));
    assert_eq!(
        scoped.1["active_kb"].as_str(),
        Some("session-kb"),
        "the deprecated mirror must track the primary for one release"
    );

    // The machine scope is untouched, and a session that never overrode inherits it.
    let machine = get_active(&app, None).await;
    assert_eq!(machine["primary_kb"].as_str(), Some("act"));
    let other = get_active(&app, Some("session-b")).await;
    assert_eq!(other["primary_kb"].as_str(), Some("act"));
    assert_eq!(other["kb_ids"], serde_json::json!(["act", "session-kb"]));
}

async fn create_bases(app: &Router, ids: &[&str]) {
    for id in ids {
        let body = serde_json::to_vec(&serde_json::json!({"id": id, "name": id})).unwrap();
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/bases")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(res.status().is_success(), "failed to create base {id}");
    }
}

/// The deprecated `kb_id` alias exists so a renderer bundle built before
/// `primary_kb` keeps working against a fresh daemon. That bundle's *only*
/// spelling for "forget the primary" is `kb_id: null` — it always sends the
/// field, and sends `null` to clear (the pre-branch route read
/// `body.kb_id.as_deref()` straight into the setter, where `None` meant clear).
///
/// Typed `Option<String>`, an explicit `null` and an omitted field both arrive
/// as `None`, so the alias could express every value except the one it was
/// kept for: a stale bundle's clear became a silent no-op, and the generated
/// TypeScript still advertised `kb_id?: string | null` as if it worked.
#[tokio::test]
async fn the_deprecated_alias_distinguishes_an_explicit_null_from_an_omission() {
    let (_d, app) = build_test_router();
    create_bases(&app, &["alpha", "beta"]).await;

    let set = post_active(&app, serde_json::json!({"kb_id": "alpha"})).await;
    assert_eq!(set.0, 200);
    assert_eq!(set.1["primary_kb"].as_str(), Some("alpha"));

    let cleared = post_active(&app, serde_json::json!({"kb_id": null})).await;
    assert_eq!(cleared.0, 200);
    assert!(
        cleared.1["primary_kb"].is_null(),
        "an explicit null on the deprecated alias must clear, as it always did"
    );
    assert!(cleared.1["active_kb"].is_null());

    // And an omitted alias still means "leave the pointer alone", so a modern
    // set-only edit does not clear the primary as a side effect.
    post_active(&app, serde_json::json!({"kb_id": "alpha"})).await;
    let set_only = post_active(&app, serde_json::json!({"hidden_kbs": ["beta"]})).await;
    assert_eq!(
        set_only.1["primary_kb"].as_str(),
        Some("alpha"),
        "an absent alias is not a clear"
    );
}

/// A 400 must mean *nothing happened*. The two halves of this body are applied
/// together — the set first, so the primary can be validated against the state
/// the request produces — and the write used to be ordered before the
/// validation, so "hide beta, and point at a base that does not exist" returned
/// an error while the hide stuck and the stored pointer was left outside the
/// resulting set. A caller that treats 400 as "my request was ignored", which
/// is the only reasonable reading, then held a stale picture of the session.
///
/// The service is where the ordering was fixed (decide-validate-commit); this
/// test pins the property at the surface a client actually sees, across both
/// scopes and both ways a body can be rejected.
#[tokio::test]
async fn a_rejected_post_leaves_the_selection_exactly_as_it_was() {
    let (_d, app) = build_test_router();
    create_bases(&app, &["alpha", "beta"]).await;

    for session in [None, Some("s1")] {
        let mut base = serde_json::json!({"hidden_kbs": [], "primary_kb": "beta"});
        if let Some(sid) = session {
            base["session_id"] = serde_json::json!(sid);
        }
        let before = post_active(&app, base).await;
        assert_eq!(before.0, 200);
        assert_eq!(before.1["primary_kb"].as_str(), Some("beta"));

        // Rejected two ways: a primary outside the resulting set, and a
        // malformed id in the set itself. Both bodies also carry a hide that
        // must not survive the rejection.
        for bad in [
            serde_json::json!({"hidden_kbs": ["beta"], "primary_kb": "ghost"}),
            serde_json::json!({"hidden_kbs": ["beta", "../escape"]}),
        ] {
            let mut bad = bad;
            if let Some(sid) = session {
                bad["session_id"] = serde_json::json!(sid);
            }
            let rejected = post_active(&app, bad.clone()).await;
            assert_eq!(rejected.0, 400, "expected a rejection for {bad}");

            let after = get_active(&app, session).await;
            assert_eq!(
                after["kb_ids"],
                serde_json::json!(["alpha", "beta"]),
                "a rejected request must not narrow the set ({bad})"
            );
            assert_eq!(
                after["hidden_kbs"],
                serde_json::json!([]),
                "a rejected request must not persist its hide ({bad})"
            );
            assert_eq!(
                after["primary_kb"].as_str(),
                Some("beta"),
                "a rejected request must not move the pointer ({bad})"
            );
        }
    }
}

/// The merged model's one invariant, at the wire. A primary that is not in the
/// resulting set is rejected with both halves named; the un-hide and the
/// re-point travel in ONE body so the GUI's "make primary" on an off row is a
/// single request validated against the state it produces.
#[tokio::test]
async fn primary_must_be_a_member_of_the_resulting_set() {
    let (_d, app) = build_test_router();
    for id in ["alpha", "beta"] {
        let create_body = serde_json::to_vec(&serde_json::json!({"id": id, "name": id})).unwrap();
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
    }

    let bad = post_active(
        &app,
        serde_json::json!({"primary_kb": "beta", "hidden_kbs": ["beta"]}),
    )
    .await;
    assert_eq!(bad.0, 400, "a hidden base cannot be the primary");

    let good = post_active(
        &app,
        serde_json::json!({"primary_kb": "beta", "hidden_kbs": []}),
    )
    .await;
    assert_eq!(good.0, 200);
    assert_eq!(good.1["primary_kb"].as_str(), Some("beta"));
}

/// A set-only edit must never move the pointer, and hiding the primary must
/// promote deterministically rather than leaving a dangling write target.
#[tokio::test]
async fn set_only_edit_keeps_the_primary_until_it_leaves_the_set() {
    let (_d, app) = build_test_router();
    for id in ["alpha", "beta", "gamma"] {
        let create_body = serde_json::to_vec(&serde_json::json!({"id": id, "name": id})).unwrap();
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
    }
    post_active(&app, serde_json::json!({"primary_kb": "beta"})).await;

    let narrowed = post_active(&app, serde_json::json!({"hidden_kbs": ["gamma"]})).await;
    assert_eq!(narrowed.1["primary_kb"].as_str(), Some("beta"));

    let orphaned = post_active(&app, serde_json::json!({"hidden_kbs": ["beta"]})).await;
    assert_eq!(
        orphaned.1["primary_kb"].as_str(),
        Some("alpha"),
        "hiding the primary promotes to the first remaining member"
    );
}
