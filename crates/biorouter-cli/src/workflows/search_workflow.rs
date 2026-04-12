use anyhow::Result;
use biorouter::config::Config;
use biorouter::workflow::read_workflow_file_content::WorkflowFile;

use super::github_workflow::{
    list_github_workflows, retrieve_workflow_from_github, WorkflowInfo, WorkflowSource,
    BIOROUTER_WORKFLOW_GITHUB_REPO_CONFIG_KEY,
};
use biorouter::workflow::local_workflows::{list_local_workflows, load_local_workflow_file};

pub fn load_workflow_file(workflow_name: &str) -> Result<WorkflowFile> {
    load_local_workflow_file(workflow_name).or_else(|e| {
        if let Some(workflow_repo_full_name) = configured_github_workflow_repo() {
            retrieve_workflow_from_github(workflow_name, &workflow_repo_full_name)
        } else {
            Err(e)
        }
    })
}

fn configured_github_workflow_repo() -> Option<String> {
    let config = Config::global();
    match config.get_param(BIOROUTER_WORKFLOW_GITHUB_REPO_CONFIG_KEY) {
        Ok(Some(workflow_repo_full_name)) => Some(workflow_repo_full_name),
        _ => None,
    }
}

/// Lists all available workflows from local paths and GitHub repositories
pub fn list_available_workflows() -> Result<Vec<WorkflowInfo>> {
    let mut workflows = Vec::new();

    // Search local workflows
    if let Ok(local_workflows) = list_local_workflows() {
        workflows.extend(local_workflows.into_iter().map(|(path, workflow)| {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            WorkflowInfo {
                name,
                source: WorkflowSource::Local,
                path: path.display().to_string(),
                title: Some(workflow.title),
                description: Some(workflow.description),
            }
        }));
    }

    // Search GitHub workflows if configured
    if let Some(repo) = configured_github_workflow_repo() {
        if let Ok(github_workflows) = list_github_workflows(&repo) {
            workflows.extend(github_workflows);
        }
    }

    Ok(workflows)
}
