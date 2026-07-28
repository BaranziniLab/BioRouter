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

/// Why a directory is being scanned — which is what decides how loudly a file
/// in it that fails to load should be reported.
///
/// Variant order is load-bearing: `WorkflowLibrary` sorts first so that a
/// directory reached both ways keeps the stricter origin through `dedup_by`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ScanOrigin {
    /// A directory that exists to hold workflows: `~/.config/biorouter/workflows`,
    /// `<project>/.biorouter/workflows`, or an entry of `BIOROUTER_WORKFLOW_PATH`.
    /// Intent is established by *location*, so any `.yaml`/`.json` in it that
    /// does not load is a broken workflow and worth an error.
    WorkflowLibrary,
    /// The process working directory, scanned opportunistically. It belongs to
    /// the user, not to Biorouter: nearly every `.yaml`/`.json` in it — package
    /// manifests, lockfiles, CI config — has nothing to do with workflows, so
    /// intent has to be established from the *content* instead.
    WorkingDirectory,
}

fn local_workflow_dirs() -> Vec<(PathBuf, ScanOrigin)> {
    collect_workflow_dirs(
        env::var(BIOROUTER_WORKFLOW_PATH_ENV_VAR).ok().as_deref(),
        get_workflow_library_dir(true),
        get_workflow_library_dir(false),
    )
}

/// The env-free half of [`local_workflow_dirs`], so the origin tagging can be
/// tested without mutating process state.
fn collect_workflow_dirs(
    workflow_path_env: Option<&str>,
    global_library: PathBuf,
    project_library: PathBuf,
) -> Vec<(PathBuf, ScanOrigin)> {
    let mut local_dirs = vec![(PathBuf::from("."), ScanOrigin::WorkingDirectory)];

    if let Some(workflow_path_env) = workflow_path_env {
        let path_separator = if cfg!(windows) { ';' } else { ':' };
        local_dirs.extend(
            workflow_path_env
                .split(path_separator)
                .map(|dir| (PathBuf::from(dir), ScanOrigin::WorkflowLibrary)),
        );
    }
    local_dirs.push((global_library, ScanOrigin::WorkflowLibrary));
    local_dirs.push((project_library, ScanOrigin::WorkflowLibrary));

    let mut dirs: Vec<(PathBuf, ScanOrigin)> = local_dirs
        .into_iter()
        .map(|(dir, origin)| (dir.canonicalize().unwrap_or(dir), origin))
        .collect();
    dirs.sort();
    // Keep one entry per path. Sorting put `WorkflowLibrary` first within a
    // path, so a workflow library that also happens to be the working directory
    // is still treated as a workflow library.
    dirs.dedup_by(|a, b| a.0 == b.0);
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
    for (dir, _origin) in &search_dirs {
        if let Ok(result) = load_workflow_file_from_dir(dir, workflow_name) {
            return Ok(result);
        }
    }

    let search_dirs_str = search_dirs
        .iter()
        .map(|(dir, _origin)| dir.display().to_string())
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
    for (dir, origin) in local_workflow_dirs() {
        if let Ok(dir_workflows) = scan_directory_for_workflows(&dir, origin) {
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

fn scan_directory_for_workflows(
    dir: &Path,
    origin: ScanOrigin,
) -> Result<Vec<(PathBuf, Workflow)>> {
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
                        Err(e) => report_workflow_load_failure(&path, &e, origin),
                    }
                }
            }
        }
    }

    Ok(workflows)
}

/// Keys that only a workflow document plausibly carries, used to establish
/// intent when location cannot — i.e. in the working directory, which is full
/// of other people's `.yaml`/`.json`.
///
/// Deliberately excluded are `version`, `title` and `description`: every
/// package manifest, lockfile and CI config in existence carries some
/// combination of them, and `ui/desktop/package.json` in this very repo carries
/// `version` *and* `description`. Requiring two such generic keys does not make
/// them evidence — it only makes the false positive need one more line of JSON.
const WORKFLOW_INTENT_KEYS: &[&str] = &["instructions", "prompt", "activities", "sub_workflows"];

/// Whether a document in an arbitrary directory declares itself a workflow.
///
/// The `workflow:` envelope is a declaration on its own — it is the key
/// `Workflow::from_content` branches on. Otherwise at least one distinctive
/// workflow key must be present.
fn declares_workflow_intent(value: &serde_yaml::Value) -> bool {
    let serde_yaml::Value::Mapping(map) = value else {
        return false;
    };
    if let Some(nested) = map.get(serde_yaml::Value::from("workflow")) {
        if nested.is_mapping() {
            return true;
        }
    }
    WORKFLOW_INTENT_KEYS
        .iter()
        .any(|key| map.contains_key(serde_yaml::Value::from(*key)))
}

/// Log a file that did not load as a workflow at the level it deserves.
///
/// A file that was never a workflow is not a failed workflow: it is skipped at
/// `debug`. `error` is reserved for a document someone meant to be a workflow
/// and that still does not load — the case someone actually needs to see.
/// Intent comes from the directory when the directory is ours, and from the
/// document's own keys when it is not.
fn report_workflow_load_failure(path: &Path, error: &anyhow::Error, origin: ScanOrigin) {
    let is_broken_workflow = match origin {
        ScanOrigin::WorkflowLibrary => true,
        ScanOrigin::WorkingDirectory => fs::read_to_string(path).map_or(true, |content| {
            // Unparseable content is reported as an error: a `.yaml`/`.json` file
            // that is not even valid YAML/JSON is broken, whoever owns it.
            serde_yaml::from_str::<serde_yaml::Value>(&content)
                .map_or(true, |value| declares_workflow_intent(&value))
        }),
    };

    if is_broken_workflow {
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

    fn scan_with_logs(dir: &Path, origin: ScanOrigin) -> (Vec<(PathBuf, Workflow)>, String) {
        let logs = CapturedLogs::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(logs.clone())
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .finish();
        let workflows = tracing::subscriber::with_default(subscriber, || {
            scan_directory_for_workflows(dir, origin).unwrap()
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

        let (workflows, logs) = scan_with_logs(dir.path(), ScanOrigin::WorkingDirectory);

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
    fn a_package_manifest_in_the_working_directory_is_not_a_broken_workflow() {
        let dir = tempfile::tempdir().unwrap();
        // An ordinary npm manifest — `ui/desktop/package.json` is the one in
        // this repo. It carries `version` and `description` like almost every
        // package manifest on earth, and nothing that is workflow-specific.
        write_file(
            dir.path(),
            "package.json",
            r#"{"name": "biorouter", "version": "1.88.5", "description": "an app", "scripts": {}}"#,
        );

        let (workflows, logs) = scan_with_logs(dir.path(), ScanOrigin::WorkingDirectory);

        assert!(workflows.is_empty());
        assert!(
            !logs.contains("ERROR"),
            "generic keys shared by every config file are not workflow intent; logs were:\n{logs}"
        );
        assert!(
            logs.contains("DEBUG") && logs.contains("package.json"),
            "the skip should still be recorded at debug; logs were:\n{logs}"
        );
    }

    #[test]
    fn an_unrelated_file_in_a_workflow_directory_is_a_broken_workflow() {
        let dir = tempfile::tempdir().unwrap();
        // The same manifest, but sitting in ~/.config/biorouter/workflows or a
        // BIOROUTER_WORKFLOW_PATH entry. Nothing but workflows belongs there,
        // so location settles intent and content never gets a vote.
        write_file(
            dir.path(),
            "package.json",
            r#"{"name": "biorouter", "version": "1.88.5", "description": "an app", "scripts": {}}"#,
        );

        let (workflows, logs) = scan_with_logs(dir.path(), ScanOrigin::WorkflowLibrary);

        assert!(workflows.is_empty());
        assert!(
            logs.contains("ERROR") && logs.contains("package.json"),
            "a file that fails to load in a workflow directory is a broken workflow; logs were:\n{logs}"
        );
    }

    #[test]
    fn a_workflow_library_that_is_also_the_working_directory_keeps_the_stricter_origin() {
        let library = tempfile::tempdir().unwrap();
        let cwd = env::current_dir().unwrap().canonicalize().unwrap();

        // The project library resolves to the working directory itself, so the
        // same path arrives twice with two different origins.
        let dirs = collect_workflow_dirs(None, library.path().to_path_buf(), cwd.clone());

        let origins: Vec<ScanOrigin> = dirs
            .iter()
            .filter(|(dir, _)| *dir == cwd)
            .map(|(_, origin)| *origin)
            .collect();
        assert_eq!(
            origins,
            vec![ScanOrigin::WorkflowLibrary],
            "dedup must keep one entry, and it must be the stricter one; dirs were {dirs:?}"
        );
    }

    #[test]
    fn only_the_working_directory_is_scanned_as_an_arbitrary_directory() {
        let global = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let on_path = tempfile::tempdir().unwrap();
        let path_env = on_path.path().display().to_string();

        let dirs = collect_workflow_dirs(
            Some(&path_env),
            global.path().to_path_buf(),
            project.path().to_path_buf(),
        );

        let ambient: Vec<&PathBuf> = dirs
            .iter()
            .filter(|(_, origin)| *origin == ScanOrigin::WorkingDirectory)
            .map(|(dir, _)| dir)
            .collect();
        assert_eq!(
            ambient,
            vec![&env::current_dir().unwrap().canonicalize().unwrap()]
        );
        for dir in [global.path(), project.path(), on_path.path()] {
            let canonical = dir.canonicalize().unwrap();
            assert!(
                dirs.contains(&(canonical.clone(), ScanOrigin::WorkflowLibrary)),
                "{} should be a workflow library; dirs were {dirs:?}",
                canonical.display()
            );
        }
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

        let (workflows, logs) = scan_with_logs(dir.path(), ScanOrigin::WorkingDirectory);

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

        let (workflows, logs) = scan_with_logs(dir.path(), ScanOrigin::WorkingDirectory);

        assert_eq!(workflows.len(), 1);
        assert_eq!(workflows[0].1.title, "Good");
        assert!(!logs.contains("ERROR"), "logs were:\n{logs}");
    }
}
