//! Routes for the managed llama-server sidecar backing the "Llama Server"
//! local provider. The desktop onboarding card and settings UI use these to
//! show sidecar state, kick off a model download/start, and stop the server.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use biorouter::providers::llamacpp::{resolve_hf_spec, MODEL_CATALOG};
use biorouter::providers::llamacpp_sidecar::{self, SidecarStatus};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::state::AppState;

/// One curated local model, as shown in the GUI/TUI pickers.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct LlamaCppModel {
    pub name: String,
    pub display_name: String,
    pub hf_spec: String,
    pub download_size: String,
    pub description: String,
    /// True for the model Biorouter preselects.
    pub is_default: bool,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct LlamaCppStatusResponse {
    pub sidecar: SidecarStatus,
    pub catalog: Vec<LlamaCppModel>,
}

#[derive(Deserialize, ToSchema)]
pub struct LlamaCppEnsureRequest {
    /// Catalog model name (e.g. `qwen3.5-4b`) or raw Hugging Face spec.
    pub model: String,
}

fn catalog() -> Vec<LlamaCppModel> {
    MODEL_CATALOG
        .iter()
        .map(|e| LlamaCppModel {
            name: e.name.to_string(),
            display_name: e.display_name.to_string(),
            hf_spec: e.hf_spec.to_string(),
            download_size: e.download_size.to_string(),
            description: e.description.to_string(),
            is_default: e.name == biorouter::providers::llamacpp::LLAMACPP_DEFAULT_MODEL,
        })
        .collect()
}

#[utoipa::path(
    get,
    path = "/llamacpp/status",
    responses(
        (status = 200, description = "Sidecar status and curated model catalog", body = LlamaCppStatusResponse)
    ),
)]
async fn llamacpp_status() -> Json<LlamaCppStatusResponse> {
    Json(LlamaCppStatusResponse {
        sidecar: llamacpp_sidecar::global().status().await,
        catalog: catalog(),
    })
}

#[utoipa::path(
    post,
    path = "/llamacpp/ensure",
    request_body = LlamaCppEnsureRequest,
    responses(
        (status = 200, description = "Sidecar start initiated (poll /llamacpp/status)", body = LlamaCppStatusResponse),
        (status = 400, description = "Unknown model name"),
    ),
)]
async fn llamacpp_ensure(
    Json(req): Json<LlamaCppEnsureRequest>,
) -> Result<Json<LlamaCppStatusResponse>, (StatusCode, String)> {
    let hf_spec =
        resolve_hf_spec(&req.model).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let sidecar = llamacpp_sidecar::global();
    sidecar
        .ensure(&req.model, &hf_spec)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Wait for readiness in the background (first run downloads the model);
    // clients poll /llamacpp/status for progress.
    let model = req.model.clone();
    tokio::spawn(async move {
        if let Err(e) = llamacpp_sidecar::global()
            .wait_ready(Duration::from_secs(3600))
            .await
        {
            tracing::warn!("llama-server for {model} did not become ready: {e}");
        }
    });

    Ok(Json(LlamaCppStatusResponse {
        sidecar: sidecar.status().await,
        catalog: catalog(),
    }))
}

#[utoipa::path(
    post,
    path = "/llamacpp/stop",
    responses(
        (status = 200, description = "Sidecar stopped", body = LlamaCppStatusResponse)
    ),
)]
async fn llamacpp_stop() -> Json<LlamaCppStatusResponse> {
    let sidecar = llamacpp_sidecar::global();
    sidecar.stop().await;
    Json(LlamaCppStatusResponse {
        sidecar: sidecar.status().await,
        catalog: catalog(),
    })
}

/// Stateless router, exposed separately so tests can drive the routes
/// without constructing an `AppState`.
pub fn router() -> Router {
    Router::new()
        .route("/llamacpp/status", get(llamacpp_status))
        .route("/llamacpp/ensure", post(llamacpp_ensure))
        .route("/llamacpp/stop", post(llamacpp_stop))
}

pub fn routes(_state: Arc<AppState>) -> Router {
    router()
}
