//! Integration tests for the /llamacpp HTTP routes.
//!
//! These avoid actually spawning a llama-server (that's covered by the
//! `llamacpp_integration` tests in the `biorouter` crate); they verify the
//! status/catalog contract the GUI onboarding card depends on, and input
//! validation on /llamacpp/ensure.

use axum::{body::Body, http::Request};
use tower::ServiceExt;

fn app() -> axum::Router {
    biorouter_server::routes::llamacpp::router()
}

async fn body_json(res: axum::response::Response) -> serde_json::Value {
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn status_returns_catalog_with_default_first_class() {
    let res = app()
        .oneshot(
            Request::builder()
                .uri("/llamacpp/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let v = body_json(res).await;
    let catalog = v.get("catalog").and_then(|c| c.as_array()).unwrap();
    assert!(!catalog.is_empty(), "catalog must not be empty");

    // Exactly one default, and it is the small Qwen3.5.
    let defaults: Vec<_> = catalog
        .iter()
        .filter(|m| m.get("is_default").and_then(|d| d.as_bool()) == Some(true))
        .collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(
        defaults[0].get("name").and_then(|n| n.as_str()),
        Some("qwen3.5-4b")
    );

    // Both requested families are present.
    let names: Vec<&str> = catalog
        .iter()
        .filter_map(|m| m.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(names.iter().any(|n| n.starts_with("qwen3.5")));
    assert!(names.iter().any(|n| n.starts_with("gemma-4")));

    // Sidecar state is one of the documented values.
    let state = v
        .get("sidecar")
        .and_then(|s| s.get("state"))
        .and_then(|s| s.as_str())
        .unwrap();
    assert!(
        ["no_binary", "stopped", "starting", "ready", "error"].contains(&state),
        "unexpected sidecar state {state}"
    );
}

#[tokio::test]
async fn ensure_rejects_unknown_model() {
    let res = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/llamacpp/ensure")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"model":"gpt-4o"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn stop_is_idempotent() {
    let res = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/llamacpp/stop")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let v = body_json(res).await;
    let state = v
        .get("sidecar")
        .and_then(|s| s.get("state"))
        .and_then(|s| s.as_str())
        .unwrap();
    assert!(state == "stopped" || state == "no_binary");
}
