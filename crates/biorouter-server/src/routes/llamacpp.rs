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
use biorouter::conversation::message::Message;
use biorouter::model::ModelConfig;
use biorouter::providers::base::Provider;
use biorouter::providers::llamacpp::{
    default_model_name, resolve_hf_spec, LlamaCppProvider, MODEL_CATALOG,
};
use biorouter::providers::llamacpp_sidecar::{self, ModelCacheStatus, SidecarStatus};
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
    pub min_gpu_memory_gib: u64,
    pub recommended_gpu_memory_gib: u64,
    pub context_limit: usize,
    /// True for the model Biorouter preselects.
    pub is_default: bool,
    /// Whether the exact GGUF/quantization is already in Biorouter's llama.cpp cache.
    pub downloaded: bool,
    /// `downloaded`, `partial`, or `not_downloaded`.
    pub download_status: ModelCacheStatus,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct LlamaCppStatusResponse {
    pub sidecar: SidecarStatus,
    pub catalog: Vec<LlamaCppModel>,
    pub system: LlamaCppSystemInfo,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct LlamaCppSystemInfo {
    pub os: String,
    pub total_memory_gib: u64,
    /// Apple Silicon unified memory, or detected discrete VRAM elsewhere.
    pub accelerator_memory_gib: Option<u64>,
    /// `apple_unified`, `vram`, or `unknown_vram`.
    pub accelerator_memory_kind: String,
    pub default_context_size: usize,
}

#[derive(Deserialize, ToSchema)]
pub struct LlamaCppEnsureRequest {
    /// Catalog model name (e.g. `gemma-4-e4b`) or raw Hugging Face spec.
    pub model: String,
}

#[derive(Deserialize, ToSchema)]
pub struct LlamaCppWarmupRequest {
    /// Catalog model name (e.g. `gemma-4-e4b`) or raw Hugging Face spec.
    pub model: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct LlamaCppWarmupResponse {
    pub sidecar: SidecarStatus,
    pub output: String,
}

fn catalog() -> Vec<LlamaCppModel> {
    let default_model = default_model_name();
    MODEL_CATALOG
        .iter()
        .map(|e| {
            let download_status = llamacpp_sidecar::model_cache_status(e.hf_spec);
            LlamaCppModel {
                name: e.name.to_string(),
                display_name: e.display_name.to_string(),
                hf_spec: e.hf_spec.to_string(),
                download_size: e.download_size.to_string(),
                description: e.description.to_string(),
                min_gpu_memory_gib: e.min_gpu_memory_gib,
                recommended_gpu_memory_gib: e.recommended_gpu_memory_gib,
                context_limit: e.context_limit,
                is_default: e.name == default_model,
                downloaded: download_status == ModelCacheStatus::Downloaded,
                download_status,
            }
        })
        .collect()
}

fn system_info() -> LlamaCppSystemInfo {
    LlamaCppSystemInfo {
        os: std::env::consts::OS.to_string(),
        total_memory_gib: llamacpp_sidecar::total_memory_gib(),
        accelerator_memory_gib: llamacpp_sidecar::accelerator_memory_gib(),
        accelerator_memory_kind: llamacpp_sidecar::accelerator_memory_kind().to_string(),
        default_context_size: llamacpp_sidecar::default_context_size(),
    }
}

async fn status_response() -> LlamaCppStatusResponse {
    LlamaCppStatusResponse {
        sidecar: llamacpp_sidecar::global().status().await,
        catalog: catalog(),
        system: system_info(),
    }
}

#[utoipa::path(
    get,
    path = "/llamacpp/status",
    responses(
        (status = 200, description = "Sidecar status and curated model catalog", body = LlamaCppStatusResponse)
    ),
)]
async fn llamacpp_status() -> Json<LlamaCppStatusResponse> {
    Json(status_response().await)
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

    Ok(Json(status_response().await))
}

#[utoipa::path(
    post,
    path = "/llamacpp/warmup",
    request_body = LlamaCppWarmupRequest,
    responses(
        (status = 200, description = "Model loaded and produced a test completion", body = LlamaCppWarmupResponse),
        (status = 400, description = "Unknown model name"),
        (status = 502, description = "Model failed to produce a warm-up completion"),
    ),
)]
async fn llamacpp_warmup(
    Json(req): Json<LlamaCppWarmupRequest>,
) -> Result<Json<LlamaCppWarmupResponse>, (StatusCode, String)> {
    resolve_hf_spec(&req.model).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let model = ModelConfig::new(&req.model)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
        .with_temperature(Some(0.0))
        .with_max_tokens(Some(8));
    let provider = LlamaCppProvider::from_env(model)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (message, _) = provider
        .complete(
            "You are a local model warm-up check. Reply with OK.",
            &[Message::user().with_text("Reply with exactly OK.")],
            &[],
        )
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    let output = message.as_concat_text().trim().to_string();
    if output.is_empty() {
        return Err((
            StatusCode::BAD_GATEWAY,
            "Llama Server returned an empty warm-up completion".to_string(),
        ));
    }

    let sidecar = llamacpp_sidecar::global();
    sidecar.mark_warmed(&req.model).await;
    Ok(Json(LlamaCppWarmupResponse {
        sidecar: sidecar.status().await,
        output,
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
    Json(status_response().await)
}

/// Stateless router, exposed separately so tests can drive the routes
/// without constructing an `AppState`.
pub fn router() -> Router {
    Router::new()
        .route("/llamacpp/status", get(llamacpp_status))
        .route("/llamacpp/ensure", post(llamacpp_ensure))
        .route("/llamacpp/warmup", post(llamacpp_warmup))
        .route("/llamacpp/stop", post(llamacpp_stop))
}

pub fn routes(_state: Arc<AppState>) -> Router {
    router()
}
