use anyhow::{anyhow, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::paths::Paths;
use crate::workflow::read_workflow_file_content::{read_workflow_file, WorkflowFile};
use crate::workflow::Workflow;
use crate::workflow::WORKFLOW_FILE_EXTENSIONS;

const BIOROUTER_WORKFLOW_PATH_ENV_VAR: &str = "BIOROUTER_WORKFLOW_PATH";

pub fn get_workflow_library_dir(is_global: bool) -> PathBuf {
    if is_global {
        Paths::config_dir().join("workflows")
    } else {
        env::current_dir().unwrap().join(".biorouter/workflows")
    }
}

fn local_workflow_dirs() -> Vec<PathBuf> {
    let mut local_dirs = vec![PathBuf::from(".")];

    if let Ok(workflow_path_env) = env::var(BIOROUTER_WORKFLOW_PATH_ENV_VAR) {
        let path_separator = if cfg!(windows) { ';' } else { ':' };
        local_dirs.extend(workflow_path_env.split(path_separator).map(PathBuf::from));
    }
    local_dirs.push(get_workflow_library_dir(true));
    local_dirs.push(get_workflow_library_dir(false));

    let mut dirs: Vec<PathBuf> = local_dirs
        .into_iter()
        .map(|dir| dir.canonicalize().unwrap_or(dir))
        .collect();
    dirs.sort();
    dirs.dedup();
    dirs
}

pub fn load_local_workflow_file(workflow_name: &str) -> Result<WorkflowFile> {
    if WORKFLOW_FILE_EXTENSIONS
        .iter()
        .any(|ext| workflow_name.ends_with(&format!(".{}", ext)))
    {
        let path = PathBuf::from(workflow_name);
        return read_workflow_file(path);
    }

    if is_file_path(workflow_name) || is_file_name(workflow_name) {
        return Err(anyhow!(
            "Workflow file {} is not a json or yaml file",
            workflow_name
        ));
    }

    let search_dirs = local_workflow_dirs();
    for dir in &search_dirs {
        if let Ok(result) = load_workflow_file_from_dir(dir, workflow_name) {
            return Ok(result);
        }
    }

    let search_dirs_str = search_dirs
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(":");
    Err(anyhow!(
        "ℹ️  Failed to retrieve {}.yaml or {}.json in {}",
        workflow_name,
        workflow_name,
        search_dirs_str
    ))
}

pub fn list_local_workflows() -> Result<Vec<(PathBuf, Workflow)>> {
    let mut workflows = Vec::new();
    for dir in local_workflow_dirs() {
        if let Ok(dir_workflows) = scan_directory_for_workflows(&dir) {
            workflows.extend(dir_workflows);
        }
    }

    Ok(workflows)
}

fn is_file_path(workflow_name: &str) -> bool {
    workflow_name.contains('/')
        || workflow_name.contains('\\')
        || workflow_name.starts_with('~')
        || workflow_name.starts_with('.')
}

fn is_file_name(workflow_name: &str) -> bool {
    Path::new(workflow_name).extension().is_some()
}

fn load_workflow_file_from_dir(dir: &Path, workflow_name: &str) -> Result<WorkflowFile> {
    for ext in WORKFLOW_FILE_EXTENSIONS {
        let workflow_path = dir.join(format!("{}.{}", workflow_name, ext));
        if let Ok(result) = read_workflow_file(workflow_path) {
            return Ok(result);
        }
    }
    Err(anyhow!(format!(
        "No {}.yaml or {}.json workflow file found in directory: {}",
        workflow_name,
        workflow_name,
        dir.display()
    )))
}

fn scan_directory_for_workflows(dir: &Path) -> Result<Vec<(PathBuf, Workflow)>> {
    let mut workflows = Vec::new();

    if !dir.exists() || !dir.is_dir() {
        return Ok(workflows);
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Some(extension) = path.extension() {
                if WORKFLOW_FILE_EXTENSIONS.contains(&extension.to_string_lossy().as_ref()) {
                    match Workflow::from_file_path(&path) {
                        Ok(workflow) => workflows.push((path.clone(), workflow)),
                        Err(e) => {
                            let error_message = format!(
                                "Failed to load workflow from file {}: {}",
                                path.display(),
                                e
                            );
                            tracing::error!("{}", error_message);
                        }
                    }
                }
            }
        }
    }

    Ok(workflows)
}

fn generate_workflow_filename(title: &str, workflow_library_dir: &Path) -> PathBuf {
    let base_name = title
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '-')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-");

    let filename = if base_name.is_empty() {
        "untitled-workflow".to_string()
    } else {
        base_name
    };

    let mut candidate = workflow_library_dir.join(format!("{}.yaml", filename));
    if !candidate.exists() {
        return candidate;
    }

    let mut counter = 1;
    loop {
        candidate = workflow_library_dir.join(format!("{}-{}.yaml", filename, counter));
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

pub fn save_workflow_to_file(workflow: Workflow, file_path: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    let workflow_library_dir = get_workflow_library_dir(true);

    let file_path_value = match file_path {
        Some(path) => path,
        None => generate_workflow_filename(&workflow.title, &workflow_library_dir),
    };

    if let Some(parent) = file_path_value.parent() {
        fs::create_dir_all(parent)?;
    }

    let yaml_content = workflow.to_yaml()?;
    fs::write(&file_path_value, yaml_content)?;
    Ok(file_path_value)
}
