//! Integration tests for the /llamacpp HTTP routes.
//!
//! These avoid actually spawning a llama-server (that's covered by the
//! `llamacpp_integration` tests in the `biorouter` crate); they verify the
//! status/catalog contract the GUI onboarding card depends on, and input
//! validation on /llamacpp/ensure and /llamacpp/warmup.

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

    // Exactly one default, chosen from the machine-tiered Ollama-linked catalog.
    let defaults: Vec<_> = catalog
        .iter()
        .filter(|m| m.get("is_default").and_then(|d| d.as_bool()) == Some(true))
        .collect();
    assert_eq!(defaults.len(), 1);
    let default_name = defaults[0].get("name").and_then(|n| n.as_str()).unwrap();
    // The 35B MoE (qwen3.6) is never a tier default — it is an explicit
    // opt-in "large" model (issue #35).
    let allowed_defaults = ["gemma4", "gemma4-12b"];
    assert!(allowed_defaults.contains(&default_name));
    assert!(defaults[0]
        .get("context_limit")
        .and_then(|n| n.as_u64())
        .is_some_and(|n| n >= 65_536));

    let default_context_size = v
        .get("system")
        .and_then(|s| s.get("default_context_size"))
        .and_then(|n| n.as_u64())
        .unwrap();
    assert!(default_context_size >= 32_768);
    assert!(
        v.get("system")
            .and_then(|s| s.get("accelerator_memory_kind"))
            .and_then(|k| k.as_str())
            .is_some(),
        "system info should expose whether recommendations use unified memory or VRAM"
    );
    assert!(
        v.get("system")
            .and_then(|s| s.get("model_cache_dir"))
            .and_then(|p| p.as_str())
            .is_some_and(|p| p.ends_with(".ollama/models") || !p.is_empty()),
        "system info should expose the Ollama-compatible model store"
    );
    assert!(
        catalog.iter().all(|m| m
            .get("official_url")
            .and_then(|url| url.as_str())
            .is_some_and(|url| url.starts_with("https://ollama.com/library/"))),
        "catalog entries should expose the official Ollama library URL"
    );
    assert!(
        catalog.iter().all(|m| m
            .get("family")
            .and_then(|family| family.as_str())
            .is_some_and(|family| ["Gemma 4", "Qwen3.6"].contains(&family))),
        "catalog entries should expose their model family"
    );
    assert!(
        catalog.iter().all(|m| m
            .get("recommended_gpu_memory_gib")
            .and_then(|n| n.as_u64())
            .is_some()),
        "catalog entries should expose GPU-addressable memory recommendations"
    );
    assert!(
        catalog.iter().all(|m| m
            .get("speed_hint")
            .and_then(|hint| hint.as_str())
            .is_some_and(|hint| !hint.trim().is_empty())),
        "catalog entries should expose a human-readable expected-speed hint"
    );
    assert!(
        catalog.iter().all(|m| m
            .get("active_params_b")
            .and_then(|n| n.as_u64())
            .is_some_and(|n| n > 0)),
        "catalog entries should expose active parameters per token"
    );
    assert!(
        catalog.iter().all(|m| m
            .get("downloaded")
            .and_then(|downloaded| downloaded.as_bool())
            .is_some()),
        "catalog entries should expose whether the model is already downloaded"
    );
    assert!(
        catalog.iter().all(|m| m
            .get("download_status")
            .and_then(|status| status.as_str())
            .is_some_and(|status| ["downloaded", "partial", "not_downloaded"].contains(&status))),
        "catalog entries should expose a documented download status"
    );
    assert!(
        catalog.iter().all(|m| m
            .get("download_source")
            .and_then(|status| status.as_str())
            .is_some_and(|status| ["ollama", "huggingface_cache", "none"].contains(&status))),
        "catalog entries should expose where any local copy was found"
    );
    assert!(
        catalog.iter().all(|m| m
            .get("fallback_downloaded")
            .and_then(|downloaded| downloaded.as_bool())
            .is_some()),
        "catalog entries should expose whether the llama.cpp fallback is cached"
    );
    assert!(
        catalog.iter().all(|m| m
            .get("fallback_download_status")
            .and_then(|status| status.as_str())
            .is_some_and(|status| ["downloaded", "partial", "not_downloaded"].contains(&status))),
        "catalog entries should expose fallback download status"
    );
    assert!(
        catalog.iter().all(|m| m
            .get("suitability_status")
            .and_then(|status| status.as_str())
            .is_some_and(
                |status| ["suitable", "above_recommendation", "unknown_resources"]
                    .contains(&status)
            )),
        "catalog entries should expose a documented suitability status"
    );

    // Both requested families are present.
    let names: Vec<&str> = catalog
        .iter()
        .filter_map(|m| m.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(names.contains(&"qwen3.6"));
    assert!(names.contains(&"gemma4"));
    assert!(!names.iter().any(|n| n.starts_with("qwen3.5")));
    let gemma4 = catalog
        .iter()
        .find(|m| m.get("name").and_then(|n| n.as_str()) == Some("gemma4"))
        .expect("Gemma 4 should remain in the catalog");
    assert_eq!(
        gemma4.get("ollama_name").and_then(|spec| spec.as_str()),
        Some("gemma4:latest")
    );
    assert_eq!(
        gemma4.get("hf_spec").and_then(|spec| spec.as_str()),
        Some("google/gemma-4-E4B-it-qat-q4_0-gguf:Q4_0")
    );

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
    assert!(
        v.get("sidecar")
            .and_then(|s| s.get("warmed"))
            .and_then(|w| w.as_bool())
            .is_some(),
        "sidecar status should expose warmed state"
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
async fn warmup_rejects_unknown_model_before_starting_sidecar() {
    let res = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/llamacpp/warmup")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"model":"gpt-4o"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn delete_rejects_unknown_model() {
    let res = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/llamacpp/delete")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"model":"gpt-4o"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn delete_removes_cached_huggingface_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = tmp.path().to_string_lossy().into_owned();
    let _guard = env_lock::lock_env([("OLLAMA_MODELS", Some(tmp_path.as_str()))]);

    let repo_dir = tmp
        .path()
        .join("models--google--gemma-4-E4B-it-qat-q4_0-gguf");
    let snapshot_dir = repo_dir.join("snapshots").join("rev");
    std::fs::create_dir_all(&snapshot_dir).unwrap();
    std::fs::write(snapshot_dir.join("gemma-4-E4B-Q4_0.gguf"), b"model").unwrap();

    let res = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/llamacpp/delete")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"model":"gemma4"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let v = body_json(res).await;
    assert_eq!(
        v.get("deleted_fallback_cache")
            .and_then(|deleted| deleted.as_bool()),
        Some(true)
    );
    assert!(!repo_dir.exists());
}

#[tokio::test]
async fn stop_is_idempotent() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    drop(listener);
    let _guard = env_lock::lock_env([("LLAMACPP_PORT", Some(port.as_str()))]);

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
