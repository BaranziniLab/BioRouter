//! `GET /catalog/*` — how a client learns the extension catalogue moved (#112).
//!
//! One long poll, deliberately, rather than a second socket. Every inventory in
//! the app already refetches over HTTP; what was missing was the *signal* to do
//! it, and a parked GET delivers that with no new transport, no reconnect logic
//! of its own, and no ordering to keep in step with the session stream.
//!
//! A daemon restart is handled by the shape rather than by a special case: the
//! revision resets to 0, so a client holding a higher number sees a *lower* one
//! come back and refetches.

use std::sync::Arc;
use std::time::Duration;

use axum::{extract::Query, extract::State, routing::get, Json, Router};
use biorouter::catalog::{CatalogDelta, CatalogEvents};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::state::AppState;

/// The longest a poll parks before answering with "nothing yet".
///
/// Under the 30s most proxies and clients give up at, and long enough that an
/// idle app is not re-establishing a request every few seconds.
const MAX_WAIT: Duration = Duration::from_secs(25);

#[derive(Debug, Deserialize, IntoParams)]
pub struct CatalogChangesQuery {
    /// The last revision this client applied. `0` (or absent) means "tell me
    /// the current revision", which is how a fresh client establishes a
    /// baseline without a refetch.
    #[serde(default)]
    since: u64,
    /// How long to park, in milliseconds. Clamped to [`MAX_WAIT`].
    #[serde(default)]
    timeout_ms: Option<u64>,
}

/// Wait for the extension/skill catalogue to change, then say what changed.
///
/// Returns immediately when the caller is already behind. A timeout is not an
/// error: the body carries the current revision and the caller polls again.
///
/// ⚠ **`truncated` is not advisory.** It means the caller fell further behind
/// than the daemon's buffer holds, so `changes` is a partial history. Applying
/// it and believing yourself current is the stale-inventory bug this endpoint
/// exists to end, one layer down. Refetch instead.
#[utoipa::path(
    get,
    path = "/catalog/changes",
    params(CatalogChangesQuery),
    responses(
        (status = 200, description = "The catalogue delta since `since`", body = CatalogDelta),
        (status = 401, description = "Unauthorized - invalid secret key"),
    )
)]
pub async fn catalog_changes(
    State(_state): State<Arc<AppState>>,
    Query(query): Query<CatalogChangesQuery>,
) -> Json<CatalogDelta> {
    let wait = query
        .timeout_ms
        .map(Duration::from_millis)
        .unwrap_or(MAX_WAIT)
        .min(MAX_WAIT);
    Json(
        CatalogEvents::global()
            .wait_for_change(query.since, wait)
            .await,
    )
}

/// The current catalogue revision, without waiting.
#[utoipa::path(
    get,
    path = "/catalog/revision",
    responses(
        (status = 200, description = "The current revision", body = CatalogDelta),
        (status = 401, description = "Unauthorized - invalid secret key"),
    )
)]
pub async fn catalog_revision(State(_state): State<Arc<AppState>>) -> Json<CatalogDelta> {
    let events = CatalogEvents::global();
    Json(CatalogDelta {
        revision: events.revision(),
        changes: Vec::new(),
        truncated: false,
    })
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/catalog/changes", get(catalog_changes))
        .route("/catalog/revision", get(catalog_revision))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use biorouter::catalog::{CatalogChangeReason, CatalogEntryChange, CatalogExtensionChange};
    use tower::ServiceExt;

    fn row(key: &str) -> CatalogExtensionChange {
        CatalogExtensionChange {
            key: key.to_string(),
            name: key.to_string(),
            display_name: None,
            change: CatalogEntryChange::Added,
            config: None,
            enabled: true,
            bundled_skill_ids: Vec::new(),
        }
    }

    async fn body(response: axum::response::Response) -> CatalogDelta {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn get(uri: &str) -> Request<Body> {
        Request::builder()
            .uri(uri)
            .header("x-secret-key", "test-secret")
            .body(Body::empty())
            .unwrap()
    }

    /// The whole contract in one pass: a client asks from 0, learns what it
    /// missed, and comes back to a quiet answer at the revision it now holds.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_client_catches_up_and_then_waits() {
        let app = routes(AppState::new().await.unwrap());
        let events = CatalogEvents::global();
        let before = events.revision();
        events.publish(
            CatalogChangeReason::Install,
            vec![row("bioroffice")],
            vec![],
            None,
        );

        let response = app
            .clone()
            .oneshot(get(&format!(
                "/catalog/changes?since={before}&timeout_ms=50"
            )))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let delta = body(response).await;
        assert!(delta.revision > before);
        assert!(delta
            .changes
            .iter()
            .any(|c| c.extensions.iter().any(|e| e.key == "bioroffice")));

        // Caught up: the next poll parks and answers with nothing.
        let current = delta.revision;
        let response = app
            .clone()
            .oneshot(get(&format!(
                "/catalog/changes?since={current}&timeout_ms=50"
            )))
            .await
            .unwrap();
        let delta = body(response).await;
        assert!(delta.changes.is_empty());
        assert_eq!(delta.revision, current);
    }

    /// A parked poll must not outlive a proxy's patience, whatever the caller
    /// asks for.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_over_long_timeout_is_clamped() {
        let app = routes(AppState::new().await.unwrap());
        let current = CatalogEvents::global().revision();
        let started = std::time::Instant::now();
        let response = app
            .oneshot(get(&format!(
                "/catalog/changes?since={current}&timeout_ms=40"
            )))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(started.elapsed() < MAX_WAIT, "the poll ignored its timeout");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_revision_endpoint_answers_without_parking() {
        let app = routes(AppState::new().await.unwrap());
        let started = std::time::Instant::now();
        let response = app.oneshot(get("/catalog/revision")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let delta = body(response).await;
        assert!(delta.changes.is_empty());
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
