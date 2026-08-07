//! Issue #56, open question 23's posture applied to `POST
//! /knowledge/bases/{id}/tier`: a daemon that was handed no user-action key
//! refuses the control **in both directions**, for every caller including the
//! person at the keyboard, and the refusal says why.
//!
//! ⚠ This is its own test binary on purpose. The installed digest is a process
//! global (`OnceLock`), so a binary that installs a key can never afterwards
//! observe a keyless daemon — `install_user_action_digest` is `let _ = set`.
//! Nothing in this file installs one, which is exactly how `just run-server`, a
//! hand-run `biorouterd agent` and a headless deployment start. Its sibling
//! `knowledge_routes::tier_route` holds the with-a-key half.

// Redirects this binary's Biorouter data/config/state dirs at a throwaway root
// before `main`, so nothing here can open the developer's real `sessions.db`.
// The lib's copy is `#[cfg(test)]` and is NOT compiled into an integration
// binary — every one of these files declares its own. Nothing here builds an
// `AppState` today, so this is a floor rather than a fix; the guard exists
// because the next test added here would not have one.
#[path = "../src/test_sandbox.rs"]
mod test_sandbox;

use axum::{body::Body, http::Request, Router};
use biorouter_mcp::knowledge::service::KnowledgeService;
use std::sync::Arc;
use tower::ServiceExt;

const TEST_SECRET: &str = "task-29a-keyless-daemon-secret";

fn daemon_without_user_action_key() -> (tempfile::TempDir, std::path::PathBuf, Router) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let svc = Arc::new(KnowledgeService::new(root.clone()));
    let router = biorouter_server::routes::knowledge::router(svc).layer(
        axum::middleware::from_fn_with_state(
            TEST_SECRET.to_string(),
            biorouter_server::auth::check_token,
        ),
    );
    (dir, root, router)
}

async fn post_tier(app: &Router, tier: &str) -> (u16, String) {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bases/omop/tier")
                .header("content-type", "application/json")
                .header("X-Secret-Key", TEST_SECRET)
                // A real key, presented in good faith by a real person. On a
                // daemon that holds no digest there is nothing to compare it
                // against, and the answer must be the same refusal.
                .header("X-User-Action", "whatever-the-user-has")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "tier": tier })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status().as_u16();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn a_daemon_with_no_user_action_key_refuses_both_directions() {
    let (_d, root, app) = daemon_without_user_action_key();
    let svc = KnowledgeService::new(root.clone());
    svc.create_base("omop", "OMOP", None).unwrap();
    biorouter_mcp::knowledge::tier::raise_unlocked(&root, "omop", true).unwrap();

    for tier in ["public", "private"] {
        let (status, body) = post_tier(&app, tier).await;
        assert_eq!(status, 403, "the {tier} direction was admitted: {body}");
        assert!(
            body.contains("started without a user-action key"),
            "the refusal must name why the control is unavailable rather than \
             reading as a permission denial: {body}"
        );
    }

    // Refused means refused: the ratchet is where it was.
    assert!(biorouter_mcp::knowledge::tier::is_private(&root, "omop"));
}
