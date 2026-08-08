use anyhow::Result;
use biorouter::config::paths::Paths;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Structure to track project information
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// The absolute path to the project directory
    pub path: String,
    /// Last time the project was accessed
    pub last_accessed: DateTime<Utc>,
    /// Last instruction sent to biorouter (if available)
    pub last_instruction: Option<String>,
    /// Last session ID associated with this project
    pub last_session_id: Option<String>,
}

/// Structure to hold all tracked projects
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectTracker {
    projects: HashMap<String, ProjectInfo>,
}

/// Project information with path as a separate field for easier access
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfoDisplay {
    /// The absolute path to the project directory
    pub path: String,
    /// Last time the project was accessed
    pub last_accessed: DateTime<Utc>,
    /// Last instruction sent to biorouter (if available)
    pub last_instruction: Option<String>,
    /// Last session ID associated with this project
    pub last_session_id: Option<String>,
}

impl ProjectTracker {
    /// Get the path to the projects.json file
    fn get_projects_file() -> Result<PathBuf> {
        let projects_file = Paths::in_data_dir("projects.json");
        if let Some(parent) = projects_file.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        Ok(projects_file)
    }

    /// Load the project tracker from the projects.json file.
    ///
    /// ⚠ **A damaged file must not brick the command forever.** This used to
    /// `?` straight out on a parse error, so a single stray byte made
    /// `biorouter projects` fail on every invocation until someone found the
    /// file and deleted it by hand — with an error naming the file but not
    /// saying it could simply be removed. That is not hypothetical: it was
    /// found on a real machine, where a torn write had left one extra `}` after
    /// 30 KB of otherwise valid JSON.
    ///
    /// The damaged file is moved aside rather than deleted, because it is the
    /// user's own history of project directories and a recovery is usually a
    /// one-line edit. The path is printed so they can do that if they want to.
    pub fn load() -> Result<Self> {
        let projects_file = Self::get_projects_file()?;
        Self::load_from(&projects_file)
    }

    /// [`Self::load`]'s body, against an explicit path.
    ///
    /// Split out so the recovery is testable at all: `load` resolves the real
    /// data directory through process-global state, so a test of it would race
    /// every other test in the binary and touch the user's own file.
    pub(crate) fn load_from(projects_file: &Path) -> Result<Self> {
        if !projects_file.exists() {
            return Ok(ProjectTracker {
                projects: HashMap::new(),
            });
        }

        let file_content = fs::read_to_string(projects_file)?;
        match serde_json::from_str::<ProjectTracker>(&file_content) {
            Ok(tracker) => Ok(tracker),
            Err(err) => {
                let quarantine =
                    projects_file.with_extension(format!("corrupt-{}", Utc::now().timestamp()));
                let moved = fs::rename(projects_file, &quarantine).is_ok();
                eprintln!(
                    "warning: {} could not be parsed ({err}). Starting a fresh project list.",
                    projects_file.display()
                );
                if moved {
                    eprintln!(
                        "         The damaged file is kept at {}",
                        quarantine.display()
                    );
                }
                Ok(ProjectTracker {
                    projects: HashMap::new(),
                })
            }
        }
    }

    /// Save the project tracker to the projects.json file.
    ///
    /// ⚠ **Write-and-rename, not `fs::write`.** `fs::write` truncates and then
    /// writes, so a process that dies mid-write — or two Biorouter processes
    /// writing at once, which is ordinary here because the desktop app, the
    /// daemon and the CLI all touch this file — leaves a torn file that the
    /// loader above then has to recover from. `rename` within the same
    /// directory is atomic on POSIX, so a reader sees either the old file or
    /// the new one and never a half of each.
    pub fn save(&self) -> Result<()> {
        let projects_file = Self::get_projects_file()?;
        self.save_to(&projects_file)
    }

    /// [`Self::save`]'s body, against an explicit path — testable for the same
    /// reason [`Self::load_from`] is.
    pub(crate) fn save_to(&self, projects_file: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;

        // Same directory, so the rename cannot cross a filesystem boundary.
        let staging = projects_file.with_extension(format!("tmp-{}", std::process::id()));
        fs::write(&staging, json)?;
        if let Err(err) = fs::rename(&staging, projects_file) {
            let _ = fs::remove_file(&staging);
            return Err(err.into());
        }
        Ok(())
    }

    /// Update project information for the current directory
    ///
    /// # Arguments
    /// * `project_dir` - The project directory to update
    /// * `instruction` - Optional instruction that was sent to biorouter
    /// * `session_id` - Optional session ID associated with this project
    pub fn update_project(
        &mut self,
        project_dir: &Path,
        instruction: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<()> {
        let dir_str = project_dir.to_string_lossy().to_string();

        // Create or update the project entry
        let project_info = self.projects.entry(dir_str.clone()).or_insert(ProjectInfo {
            path: dir_str,
            last_accessed: Utc::now(),
            last_instruction: None,
            last_session_id: None,
        });

        // Update the last accessed time
        project_info.last_accessed = Utc::now();

        // Update the last instruction if provided
        if let Some(instr) = instruction {
            project_info.last_instruction = Some(instr.to_string());
        }

        // Update the session ID if provided
        if let Some(id) = session_id {
            project_info.last_session_id = Some(id.to_string());
        }

        self.save()
    }

    /// [`Self::update_project`] against an explicit file, for tests.
    #[cfg(test)]
    pub(crate) fn update_project_at(
        &mut self,
        projects_file: &Path,
        project_dir: &Path,
        instruction: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<()> {
        let dir_str = project_dir.to_string_lossy().to_string();
        let project_info = self.projects.entry(dir_str.clone()).or_insert(ProjectInfo {
            path: dir_str,
            last_accessed: Utc::now(),
            last_instruction: None,
            last_session_id: None,
        });
        project_info.last_accessed = Utc::now();
        if let Some(instr) = instruction {
            project_info.last_instruction = Some(instr.to_string());
        }
        if let Some(id) = session_id {
            project_info.last_session_id = Some(id.to_string());
        }
        self.save_to(projects_file)
    }

    /// List all tracked projects
    ///
    /// Returns a vector of ProjectInfoDisplay objects
    pub fn list_projects(&self) -> Vec<ProjectInfoDisplay> {
        self.projects
            .values()
            .map(|info| ProjectInfoDisplay {
                path: info.path.clone(),
                last_accessed: info.last_accessed,
                last_instruction: info.last_instruction.clone(),
                last_session_id: info.last_session_id.clone(),
            })
            .collect()
    }
}

/// Update the project tracker with the current directory and optional instruction
///
/// # Arguments
/// * `instruction` - Optional instruction that was sent to biorouter
/// * `session_id` - Optional session ID associated with this project
pub fn update_project_tracker(instruction: Option<&str>, session_id: Option<&str>) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let mut tracker = ProjectTracker::load()?;
    tracker.update_project(&current_dir, instruction, session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⚠ **Found on a real machine, not imagined.** `projects.json` had 30 KB of
    /// valid JSON followed by `"\n}"` — the tail of a longer previous write that
    /// a non-atomic `fs::write` had left behind. The old loader `?`d out, so
    /// `biorouter projects` failed on *every* invocation from then on, with an
    /// error that named the file but never said it could simply be removed.
    #[test]
    fn a_torn_file_is_moved_aside_rather_than_failing_forever() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("projects.json");
        // Exactly the shape seen in the wild: valid object, then a stray brace.
        fs::write(&path, "{\"projects\":{}}\n}").unwrap();

        let tracker = ProjectTracker::load_from(&path).expect("must not fail the command");
        assert!(tracker.list_projects().is_empty());

        // The user's own history is preserved, not deleted — recovery is usually
        // a one-line edit, and on the machine this was found on it was 111
        // project entries.
        assert!(
            !path.exists(),
            "the damaged file should have been moved aside"
        );
        let kept: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("corrupt-"))
            .collect();
        assert_eq!(kept.len(), 1, "expected exactly one quarantined file");
    }

    #[test]
    fn a_healthy_file_is_read_normally_and_left_alone() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("projects.json");
        let mut tracker = ProjectTracker {
            projects: HashMap::new(),
        };
        tracker
            .update_project_at(&path, Path::new("/tmp/demo"), None, None)
            .unwrap();

        let reloaded = ProjectTracker::load_from(&path).unwrap();
        assert_eq!(reloaded.list_projects().len(), 1);
        assert!(path.exists(), "a healthy file must not be quarantined");
    }

    /// The write is staged and renamed, so a reader never sees half a file.
    /// Asserted by its observable consequence: no `.tmp-*` left behind.
    #[test]
    fn saving_leaves_no_staging_file_behind() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("projects.json");
        let mut tracker = ProjectTracker {
            projects: HashMap::new(),
        };
        tracker
            .update_project_at(&path, Path::new("/tmp/demo"), None, None)
            .unwrap();

        let strays: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("tmp-"))
            .collect();
        assert!(strays.is_empty(), "staging file was not renamed away");
    }
}
