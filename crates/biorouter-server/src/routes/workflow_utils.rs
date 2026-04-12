use std::collections::HashMap;
use std::fs;
use std::hash::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use crate::routes::errors::ErrorResponse;
use crate::state::AppState;
use anyhow::Result;
use axum::http::StatusCode;
use biorouter::agents::Agent;
use biorouter::prompt_template::render_global_file;
use biorouter::workflow::build_workflow::{build_workflow_from_template, WorkflowError};
use biorouter::workflow::local_workflows::{get_workflow_library_dir, list_local_workflows};
use biorouter::workflow::validate_workflow::validate_workflow_template_from_content;
use biorouter::workflow::Workflow;
use serde::Serialize;
use serde_json::Value;
use tracing::error;
use utoipa::ToSchema;

pub struct WorkflowValidationError {
    pub status: StatusCode,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkflowManifest {
    pub id: String,
    pub workflow: Workflow,
    #[schema(value_type = String)]
    pub file_path: PathBuf,
    pub last_modified: String,
    pub schedule_cron: Option<String>,
    pub slash_command: Option<String>,
}

pub fn short_id_from_path(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let h = hasher.finish();
    format!("{:016x}", h)
}

pub fn get_all_workflows_manifests() -> Result<Vec<WorkflowManifest>> {
    let workflows_with_path = list_local_workflows()?;
    let mut workflow_manifests_with_path = Vec::new();
    for (file_path, workflow) in workflows_with_path {
        let Ok(last_modified) = fs::metadata(file_path.clone())
            .map(|m| chrono::DateTime::<chrono::Utc>::from(m.modified().unwrap()).to_rfc3339())
        else {
            continue;
        };

        let manifest_with_path = WorkflowManifest {
            id: short_id_from_path(file_path.to_string_lossy().as_ref()),
            workflow,
            file_path,
            last_modified,
            schedule_cron: None,
            slash_command: None,
        };
        workflow_manifests_with_path.push(manifest_with_path);
    }
    workflow_manifests_with_path.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));

    Ok(workflow_manifests_with_path)
}

pub fn validate_workflow(workflow: &Workflow) -> Result<(), WorkflowValidationError> {
    let workflow_yaml = workflow.to_yaml().map_err(|err| {
        let message = err.to_string();
        error!("Failed to serialize workflow for validation: {}", message);
        WorkflowValidationError {
            status: StatusCode::BAD_REQUEST,
            message,
        }
    })?;

    validate_workflow_template_from_content(&workflow_yaml, None).map_err(|err| {
        let message = err.to_string();
        error!("Workflow validation failed: {}", message);
        WorkflowValidationError {
            status: StatusCode::BAD_REQUEST,
            message,
        }
    })?;

    Ok(())
}

pub async fn get_workflow_file_path_by_id(
    state: &AppState,
    id: &str,
) -> Result<PathBuf, ErrorResponse> {
    let cached_path = {
        let map = state.workflow_file_hash_map.lock().await;
        map.get(id).cloned()
    };

    if let Some(path) = cached_path {
        return Ok(path);
    }

    let workflow_manifest_with_paths = get_all_workflows_manifests().unwrap_or_default();
    let mut workflow_file_hash_map = HashMap::new();
    let mut resolved_path: Option<PathBuf> = None;

    for workflow_manifest_with_path in &workflow_manifest_with_paths {
        if workflow_manifest_with_path.id == id {
            resolved_path = Some(workflow_manifest_with_path.file_path.clone());
        }
        workflow_file_hash_map.insert(
            workflow_manifest_with_path.id.clone(),
            workflow_manifest_with_path.file_path.clone(),
        );
    }

    state.set_workflow_file_hash_map(workflow_file_hash_map).await;

    resolved_path.ok_or_else(|| ErrorResponse {
        message: format!("Workflow not found: {}", id),
        status: StatusCode::NOT_FOUND,
    })
}

pub async fn load_workflow_by_id(state: &AppState, id: &str) -> Result<Workflow, ErrorResponse> {
    let path = get_workflow_file_path_by_id(state, id).await?;

    Workflow::from_file_path(&path).map_err(|err| ErrorResponse {
        message: format!("Failed to load workflow: {}", err),
        status: StatusCode::INTERNAL_SERVER_ERROR,
    })
}

pub async fn build_workflow_with_parameter_values(
    original_workflow: &Workflow,
    user_workflow_values: HashMap<String, String>,
) -> Result<Option<Workflow>> {
    let workflow_content = original_workflow.to_yaml()?;

    let workflow_dir = get_workflow_library_dir(true);
    let params = user_workflow_values.into_iter().collect();

    let workflow = match build_workflow_from_template(
        workflow_content,
        &workflow_dir,
        params,
        None::<fn(&str, &str) -> Result<String, anyhow::Error>>,
    ) {
        Ok(workflow) => Some(workflow),
        Err(WorkflowError::MissingParams { .. }) => None,
        Err(e) => return Err(anyhow::anyhow!(e)),
    };

    Ok(workflow)
}

pub async fn apply_workflow_to_agent(
    agent: &Arc<Agent>,
    workflow: &Workflow,
    include_final_output_tool: bool,
) -> Option<String> {
    agent
        .apply_workflow_components(
            workflow.sub_workflows.clone(),
            workflow.response.clone(),
            include_final_output_tool,
        )
        .await;

    workflow.instructions.as_ref().map(|instructions| {
        let mut context: HashMap<&str, Value> = HashMap::new();
        context.insert("workflow_instructions", Value::String(instructions.clone()));
        render_global_file("desktop_workflow_instruction.md", &context).expect("Prompt should render")
    })
}
