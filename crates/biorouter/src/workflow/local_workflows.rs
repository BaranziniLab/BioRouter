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
                        Err(e) => report_workflow_load_failure(&path, &e),
                    }
                }
            }
        }
    }

    Ok(workflows)
}

/// Keys that mark a document as an attempt at a workflow. The scanned
/// directories (the working directory, `~/.config/biorouter/workflows`, any
/// `BIOROUTER_WORKFLOW_PATH` entry) also hold JSON belonging to other tools —
/// `~/.claude.json` is the canonical example — and those are not failures.
const WORKFLOW_MARKER_KEYS: &[&str] = &[
    "title",
    "description",
    "instructions",
    "prompt",
    "activities",
    "parameters",
    "extensions",
    "sub_workflows",
    "version",
];

/// A document is workflow-shaped when it is a mapping carrying at least two
/// workflow keys, or a mapping with the `workflow:` envelope
/// `Workflow::from_content` accepts. One key alone is too weak a signal — a
/// great many unrelated config files carry `version` or `description`.
fn is_workflow_shaped(value: &serde_yaml::Value) -> bool {
    let serde_yaml::Value::Mapping(map) = value else {
        return false;
    };
    if let Some(nested) = map.get(serde_yaml::Value::from("workflow")) {
        if nested.is_mapping() {
            return true;
        }
    }
    let markers = WORKFLOW_MARKER_KEYS
        .iter()
        .filter(|key| map.contains_key(serde_yaml::Value::from(**key)))
        .count();
    markers >= 2
}

/// Log a file that did not load as a workflow at the level its shape deserves.
///
/// A file that was never a workflow is not a failed workflow: it is skipped at
/// `debug`. `error` is reserved for a document that looks like a workflow and
/// still does not load — the case someone actually needs to see in the log.
fn report_workflow_load_failure(path: &Path, error: &anyhow::Error) {
    let looks_like_workflow = fs::read_to_string(path).map_or(true, |content| {
        // Unparseable content is reported as an error: a `.yaml`/`.json` file
        // that is not even valid YAML/JSON is broken, whoever owns it.
        serde_yaml::from_str::<serde_yaml::Value>(&content)
            .map_or(true, |value| is_workflow_shaped(&value))
    });

    if looks_like_workflow {
        tracing::error!(
            "Failed to load workflow from file {}: {}",
            path.display(),
            error
        );
    } else {
        tracing::debug!("Skipping non-workflow file {}: {}", path.display(), error);
    }
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

pub fn save_workflow_to_file(
    workflow: Workflow,
    file_path: Option<PathBuf>,
) -> anyhow::Result<PathBuf> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    /// Collects formatted tracing output so a test can assert the *level* a
    /// message was emitted at, not merely that it was emitted.
    #[derive(Clone, Default)]
    struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

    impl CapturedLogs {
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap()).to_string()
        }
    }

    impl std::io::Write for CapturedLogs {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for CapturedLogs {
        type Writer = CapturedLogs;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    fn scan_with_logs(dir: &Path) -> (Vec<(PathBuf, Workflow)>, String) {
        let logs = CapturedLogs::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(logs.clone())
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .finish();
        let workflows = tracing::subscriber::with_default(subscriber, || {
            scan_directory_for_workflows(dir).unwrap()
        });
        (workflows, logs.text())
    }

    fn write_file(dir: &Path, name: &str, contents: &str) {
        fs::write(dir.join(name), contents).unwrap();
    }

    #[test]
    fn unrelated_json_in_a_scanned_directory_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        // Shaped like ~/.claude.json: another tool's config that happens to
        // live in a directory the workflow loader scans.
        write_file(
            dir.path(),
            "unrelated.json",
            r#"{"numStartups": 12, "installMethod": "brew", "projects": {}}"#,
        );

        let (workflows, logs) = scan_with_logs(dir.path());

        assert!(workflows.is_empty());
        assert!(
            !logs.contains("ERROR"),
            "a file that was never a workflow is not a failed workflow; logs were:\n{logs}"
        );
        assert!(
            logs.contains("DEBUG") && logs.contains("unrelated.json"),
            "the skip should still be recorded at debug; logs were:\n{logs}"
        );
    }

    #[test]
    fn a_malformed_workflow_is_still_an_error() {
        let dir = tempfile::tempdir().unwrap();
        // Carries workflow keys but is missing the required `title`.
        write_file(
            dir.path(),
            "broken.yaml",
            "description: does something\nprompt: do the thing\n",
        );

        let (workflows, logs) = scan_with_logs(dir.path());

        assert!(workflows.is_empty());
        assert!(
            logs.contains("ERROR") && logs.contains("broken.yaml"),
            "a file that looks like a workflow but does not load must be an error; logs were:\n{logs}"
        );
    }

    #[test]
    fn valid_workflows_are_still_loaded_alongside_unrelated_files() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            dir.path(),
            "good.yaml",
            "title: Good\ndescription: A good workflow\nprompt: go\n",
        );
        write_file(dir.path(), "unrelated.json", r#"{"machineID": "abc"}"#);

        let (workflows, logs) = scan_with_logs(dir.path());

        assert_eq!(workflows.len(), 1);
        assert_eq!(workflows[0].1.title, "Good");
        assert!(!logs.contains("ERROR"), "logs were:\n{logs}");
    }
}
