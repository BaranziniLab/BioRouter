//! HTTP-shaped adapters over the workflow core.
//!
//! Everything here used to be an implementation. It is now a translation layer
//! and nothing else: each function forwards to
//! [`biorouter::workflow::service`] and turns its `anyhow::Error` into the
//! `StatusCode` the route needs. That direction is forced by the crate graph —
//! `biorouter-server` depends on `biorouter`, never the reverse — so anything a
//! route and an agent tool must agree on can only live down in the core. While
//! the id resolution, validation, manifest listing and enrichment lived up here,
//! the only other caller (the CLI) could not reach them, and grew its own
//! divergent copies of all four.
//!
//! ⚠ Do not reintroduce logic here. A behaviour that exists in this file exists
//! for the HTTP surface alone, which is exactly how the surfaces drifted apart.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::routes::errors::ErrorResponse;
use anyhow::Result;
use axum::http::StatusCode;
use biorouter::agents::Agent;
use biorouter::workflow::service;
use biorouter::workflow::Workflow;
use tracing::error;

pub use biorouter::workflow::service::{short_id_from_path, WorkflowManifest};

pub struct WorkflowValidationError {
    pub status: StatusCode,
    pub message: String,
}

pub fn get_all_workflows_manifests() -> Result<Vec<WorkflowManifest>> {
    service::list_manifests()
}

pub fn validate_workflow(workflow: &Workflow) -> Result<(), WorkflowValidationError> {
    service::validate(workflow).map_err(|err| {
        let message = err.to_string();
        error!("Workflow validation failed: {}", message);
        WorkflowValidationError {
            status: StatusCode::BAD_REQUEST,
            message,
        }
    })
}

/// Resolve a workflow id to its path.
///
/// ⚠ No longer takes `AppState`. The id→path map used to hang off the shared
/// state, be replaced wholesale only by `GET /workflows/list`, and never be
/// invalidated — so a renamed or deleted workflow kept resolving, and the stale
/// path was handed to delete, schedule, slash-command and save. The cache now
/// lives in the core with root-mtime staleness and an existence check on every
/// hit, which is also the only way the CLI and the agent tool could share it.
pub fn get_workflow_file_path_by_id(id: &str) -> Result<PathBuf, ErrorResponse> {
    service::resolve_id(id).map_err(|err| ErrorResponse {
        message: err.to_string(),
        status: StatusCode::NOT_FOUND,
    })
}

pub fn load_workflow_by_id(id: &str) -> Result<Workflow, ErrorResponse> {
    let path = get_workflow_file_path_by_id(id)?;
    Workflow::from_file_path(&path).map_err(|err| ErrorResponse {
        message: format!("Failed to load workflow: {}", err),
        status: StatusCode::INTERNAL_SERVER_ERROR,
    })
}

pub async fn build_workflow_with_parameter_values(
    original_workflow: &Workflow,
    user_workflow_values: HashMap<String, String>,
) -> Result<Option<Workflow>> {
    service::build_with_parameter_values(original_workflow, user_workflow_values)
}

pub async fn apply_workflow_to_agent(
    agent: &Arc<Agent>,
    session_id: &str,
    workflow: &Workflow,
    include_final_output_tool: bool,
) -> Result<Option<String>> {
    biorouter::workflow::runtime::apply_to_agent(
        agent.as_ref(),
        session_id,
        workflow,
        include_final_output_tool,
    )
    .await
}
