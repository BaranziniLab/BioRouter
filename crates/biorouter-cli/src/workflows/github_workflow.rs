use anyhow::{anyhow, Result};
use biorouter::workflow::template_workflow::parse_workflow_content;
use biorouter::workflow::WORKFLOW_FILE_EXTENSIONS;
use console::style;
use serde::{Deserialize, Serialize};

use biorouter::workflow::read_workflow_file_content::WorkflowFile;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use tar::Archive;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInfo {
    pub name: String,
    pub source: WorkflowSource,
    pub path: String,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkflowSource {
    Local,
    GitHub,
}

pub const BIOROUTER_WORKFLOW_GITHUB_REPO_CONFIG_KEY: &str = "BIOROUTER_WORKFLOW_GITHUB_REPO";
pub fn retrieve_workflow_from_github(
    workflow_name: &str,
    workflow_repo_full_name: &str,
) -> Result<WorkflowFile> {
    println!(
        "📦 Looking for workflow \"{}\" in github repo: {}",
        workflow_name, workflow_repo_full_name
    );
    ensure_gh_authenticated()?;
    let max_attempts = 2;
    let mut last_err = None;

    for attempt in 1..=max_attempts {
        match clone_and_download_workflow(workflow_name, workflow_repo_full_name) {
            Ok(download_dir) => match read_workflow_file(&download_dir) {
                Ok((content, workflow_file_local_path)) => {
                    return Ok(WorkflowFile {
                        content,
                        parent_dir: download_dir.clone(),
                        file_path: workflow_file_local_path,
                    })
                }
                Err(err) => return Err(err),
            },
            Err(err) => {
                last_err = Some(err);
            }
        }
        if attempt < max_attempts {
            clean_cloned_dirs(workflow_repo_full_name)?;
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Unknown error occurred")))
}

fn clean_cloned_dirs(workflow_repo_full_name: &str) -> anyhow::Result<()> {
    let local_repo_path = get_local_repo_path(&env::temp_dir(), workflow_repo_full_name)?;
    if local_repo_path.exists() {
        fs::remove_dir_all(&local_repo_path)?;
    }
    Ok(())
}
fn read_workflow_file(download_dir: &Path) -> Result<(String, PathBuf)> {
    for ext in WORKFLOW_FILE_EXTENSIONS {
        let candidate_file_path = download_dir.join(format!("workflow.{}", ext));
        if candidate_file_path.exists() {
            let content = fs::read_to_string(&candidate_file_path)?;
            println!(
                "⬇️  Retrieved workflow file: {}",
                candidate_file_path
                    .strip_prefix(download_dir)
                    .unwrap()
                    .display()
            );
            return Ok((content, candidate_file_path));
        }
    }

    Err(anyhow::anyhow!(
        "No workflow file found in {} (looked for extensions: {:?})",
        download_dir.display(),
        WORKFLOW_FILE_EXTENSIONS
    ))
}

fn clone_and_download_workflow(
    workflow_name: &str,
    workflow_repo_full_name: &str,
) -> Result<PathBuf> {
    let local_repo_path = ensure_repo_cloned(workflow_repo_full_name)?;
    fetch_origin(&local_repo_path)?;
    get_folder_from_github(&local_repo_path, workflow_name)
}

pub fn ensure_gh_authenticated() -> Result<()> {
    // Check authentication status
    let status = Command::new("gh")
        .args(["auth", "status"])
        .status()
        .map_err(|_| {
            anyhow::anyhow!("Failed to run `gh auth status`. Make sure you have `gh` installed.")
        })?;

    if status.success() {
        return Ok(());
    }
    println!("GitHub CLI is not authenticated. Launching `gh auth login`...");
    // Run `gh auth login` interactively
    let login_status = Command::new("gh")
        .args(["auth", "login", "--git-protocol", "https"])
        .status()
        .map_err(|_| anyhow::anyhow!("Failed to run `gh auth login`"))?;

    if !login_status.success() {
        Err(anyhow::anyhow!("Failed to authenticate using GitHub CLI."))
    } else {
        Ok(())
    }
}

fn get_local_repo_path(
    local_repo_parent_path: &Path,
    workflow_repo_full_name: &str,
) -> Result<PathBuf> {
    let (_, repo_name) = workflow_repo_full_name
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("Invalid repository name format"))?;
    let local_repo_path = local_repo_parent_path.to_path_buf().join(repo_name);
    Ok(local_repo_path)
}

fn ensure_repo_cloned(workflow_repo_full_name: &str) -> Result<PathBuf> {
    let local_repo_parent_path = env::temp_dir();
    if !local_repo_parent_path.exists() {
        std::fs::create_dir_all(local_repo_parent_path.clone())?;
    }
    let local_repo_path = get_local_repo_path(&local_repo_parent_path, workflow_repo_full_name)?;

    if local_repo_path.join(".git").exists() {
        Ok(local_repo_path)
    } else {
        let error_message: String = format!("Failed to clone repo: {}", workflow_repo_full_name);
        let status = Command::new("gh")
            .args(["repo", "clone", workflow_repo_full_name])
            .current_dir(local_repo_parent_path.clone())
            .status()
            .map_err(|_: std::io::Error| anyhow::anyhow!(error_message.clone()))?;

        if status.success() {
            Ok(local_repo_path)
        } else {
            Err(anyhow::anyhow!(error_message))
        }
    }
}

fn fetch_origin(local_repo_path: &Path) -> Result<()> {
    let error_message: String = format!("Failed to fetch at {}", local_repo_path.to_str().unwrap());
    let status = Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(local_repo_path)
        .status()
        .map_err(|_| anyhow::anyhow!(error_message.clone()))?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(error_message))
    }
}

fn get_folder_from_github(local_repo_path: &Path, workflow_name: &str) -> Result<PathBuf> {
    let ref_and_path = format!("origin/main:{}", workflow_name);
    let output_dir = env::temp_dir().join(workflow_name);

    if output_dir.exists() {
        fs::remove_dir_all(&output_dir)?;
    }
    fs::create_dir_all(&output_dir)?;

    let archive_output = Command::new("git")
        .args(["archive", &ref_and_path])
        .current_dir(local_repo_path)
        .stdout(Stdio::piped())
        .spawn()?;

    let stdout = archive_output
        .stdout
        .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout from git archive"))?;

    let mut archive = Archive::new(stdout);
    archive.unpack(&output_dir)?;
    list_files(&output_dir)?;

    Ok(output_dir)
}

fn list_files(dir: &Path) -> Result<()> {
    println!("{}", style("Files downloaded from github:").bold());
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            println!("  - {}", path.display());
        }
    }
    Ok(())
}

/// Lists all available workflows from a GitHub repository
pub fn list_github_workflows(repo: &str) -> Result<Vec<WorkflowInfo>> {
    discover_github_workflows(repo)
}

fn discover_github_workflows(repo: &str) -> Result<Vec<WorkflowInfo>> {
    use serde_json::Value;
    use std::process::Command;

    // Ensure GitHub CLI is authenticated
    ensure_gh_authenticated()?;

    // Get repository contents using GitHub CLI
    let output = Command::new("gh")
        .args(["api", &format!("repos/{}/contents", repo)])
        .output()
        .map_err(|e| anyhow!("Failed to fetch repository contents using 'gh api' command (executed when BIOROUTER_WORKFLOW_GITHUB_REPO is configured). This requires GitHub CLI (gh) to be installed and authenticated. Error: {}", e))?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("GitHub API request failed: {}", error_msg));
    }

    let contents: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| anyhow!("Failed to parse GitHub API response: {}", e))?;

    let mut workflows = Vec::new();

    if let Some(items) = contents.as_array() {
        for item in items {
            if let (Some(name), Some(item_type)) = (
                item.get("name").and_then(|n| n.as_str()),
                item.get("type").and_then(|t| t.as_str()),
            ) {
                if item_type == "dir" {
                    // Check if this directory contains a workflow file
                    if let Ok(workflow_info) = check_github_directory_for_workflow(repo, name) {
                        workflows.push(workflow_info);
                    }
                }
            }
        }
    }

    Ok(workflows)
}

fn check_github_directory_for_workflow(repo: &str, dir_name: &str) -> Result<WorkflowInfo> {
    use serde_json::Value;
    use std::process::Command;

    // Check directory contents for workflow files
    let output = Command::new("gh")
        .args(["api", &format!("repos/{}/contents/{}", repo, dir_name)])
        .output()
        .map_err(|e| anyhow!("Failed to check directory contents: {}", e))?;

    if !output.status.success() {
        return Err(anyhow!("Failed to access directory: {}", dir_name));
    }

    let contents: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| anyhow!("Failed to parse directory contents: {}", e))?;

    if let Some(items) = contents.as_array() {
        for item in items {
            if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                if WORKFLOW_FILE_EXTENSIONS
                    .iter()
                    .any(|ext| name == format!("workflow.{}", ext))
                {
                    // Found a workflow file, get its content
                    return get_github_workflow_info(repo, dir_name, name);
                }
            }
        }
    }

    Err(anyhow!("No workflow file found in directory: {}", dir_name))
}

fn get_github_workflow_info(
    repo: &str,
    dir_name: &str,
    workflow_filename: &str,
) -> Result<WorkflowInfo> {
    use serde_json::Value;
    use std::process::Command;

    // Get the workflow file content
    let output = Command::new("gh")
        .args([
            "api",
            &format!("repos/{}/contents/{}/{}", repo, dir_name, workflow_filename),
        ])
        .output()
        .map_err(|e| anyhow!("Failed to get workflow file content: {}", e))?;

    if !output.status.success() {
        return Err(anyhow!(
            "Failed to access workflow file: {}/{}",
            dir_name,
            workflow_filename
        ));
    }

    let file_info: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| anyhow!("Failed to parse file info: {}", e))?;

    if let Some(content_b64) = file_info.get("content").and_then(|c| c.as_str()) {
        // Decode base64 content
        use base64::{engine::general_purpose, Engine as _};
        let content_bytes = general_purpose::STANDARD
            .decode(content_b64.replace('\n', ""))
            .map_err(|e| anyhow!("Failed to decode base64 content: {}", e))?;

        let content = String::from_utf8(content_bytes)
            .map_err(|e| anyhow!("Failed to convert content to string: {}", e))?;

        // Parse the workflow content
        let (workflow, _) =
            parse_workflow_content(&content, Some(format!("{}/{}", repo, dir_name)))?;

        return Ok(WorkflowInfo {
            name: dir_name.to_string(),
            source: WorkflowSource::GitHub,
            path: format!("{}/{}", repo, dir_name),
            title: Some(workflow.title),
            description: Some(workflow.description),
        });
    }

    Err(anyhow!("Failed to get workflow content from GitHub"))
}
